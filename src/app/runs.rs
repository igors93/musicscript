//! Rendering and debugging the script. [`Execution`](super::execution)
//! decides; these methods send the requests and apply the results.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use super::execution::{RunEnd, RunRequest, StopPlan};
use super::{AppState, StatusKind};
use crate::editor::text_ops::{self, Located};
use crate::editor::{LineTiming, format_seconds};
use crate::logging::{self, Level};
use crate::parameters;
use crate::scene::{Overlay, RunScript, SceneNode};
use crate::worker::protocol::{
    DebugAction, Request, RequestId, RunFinished, RunProfile, SceneNode as NodeMessage,
};

/// How long after the last parameter change the script renders again, in
/// seconds: a drag renders once it pauses, not on every pixel.
pub(super) const PARAMETER_RENDER_DELAY: f64 = 0.3;

impl AppState {
    /// Writes a parameter's new value into the script, as an undoable
    /// edit, and renders again shortly after.
    pub(super) fn set_parameter(&mut self, name: &str, line: usize, value: f64) {
        if self.is_debugging() {
            self.set_status(
                StatusKind::Warning,
                "Parameters can be changed once debugging ends",
            );
            return;
        }
        let Some(parameter) = parameters::parameters(&self.document.text)
            .into_iter()
            .find(|parameter| parameter.line == line && parameter.name == name)
        else {
            return;
        };
        let literal = parameter.literal_for(value);
        let now = self.now();
        self.editor
            .adjust_text(&mut self.document, parameter.literal, &literal, now);
        self.parameter_render_at = Some(now + PARAMETER_RENDER_DELAY);
    }

    /// Renders once the parameters stopped changing for a moment.
    pub(super) fn poll_parameter_render(&mut self, ctx: &egui::Context) {
        let Some(at) = self.parameter_render_at else {
            return;
        };
        let now = ctx.input(|input| input.time);
        if now < at {
            if let Ok(wait) = std::time::Duration::try_from_secs_f64(at - now) {
                ctx.request_repaint_after(wait);
            }
            return;
        }
        self.parameter_render_at = None;
        if !self.is_debugging() {
            self.start_run(false);
        }
    }

    pub(super) fn start_run(&mut self, debug: bool) {
        self.start_run_with(debug, false);
    }

    /// Renders with the time of each line measured, shown in the gutter.
    pub(super) fn start_profile(&mut self) {
        if self.is_busy() {
            self.set_status(StatusKind::Warning, "Already running; stop it first");
            return;
        }
        self.start_run_with(false, true);
    }

    fn start_run_with(&mut self, debug: bool, profile: bool) {
        let availability = self.worker.status().into();
        match self.execution.request_run(debug, availability) {
            RunRequest::Start => {}
            RunRequest::WhenReady => {
                self.set_status(StatusKind::Info, "Waiting for Python to start…");
                return;
            }
            RunRequest::Unavailable => {
                self.set_status(
                    StatusKind::Error,
                    "Python is not available — check Preferences > General",
                );
                return;
            }
            RunRequest::Queued => return,
            RunRequest::Busy => {
                self.set_status(StatusKind::Warning, "Already running; stop it first");
                return;
            }
        }

        // A reload may have changed the text since the editor last drew.
        self.editor.sync_markers(&self.document.text);
        let id = self.worker.next_id();
        // Pinned, so a debugging session may reference them even after it
        // clears the view below.
        let known_geometry = self.scene.known_geometry();
        self.scene.pin_geometry(id, &known_geometry);
        self.run_script = Some((
            id,
            Arc::new(RunScript {
                document: self.document_generation,
                text: self.document.text.clone(),
            }),
        ));
        let request = Request::Run {
            id,
            script: self.document.text.clone(),
            path: self.document.path().map(|p| p.display().to_string()),
            debug,
            breakpoints: self.editor.breakpoints.iter().copied().collect(),
            settings: self.settings.run_settings(),
            scene: self.settings.scene_settings(),
            known_geometry,
            profile,
        };
        if !self.worker.send(&request) {
            self.set_status(StatusKind::Error, "Cannot reach Python");
            return;
        }
        self.execution.started(id, debug);
        // The times of an earlier profile no longer describe the script.
        self.editor.line_profile.clear();
        if profile {
            self.set_status(StatusKind::Info, "Profiling…");
        }
        if debug {
            // Like the original: the view starts empty and follows the
            // locals of each paused frame.
            let removed = self.scene.clear();
            self.release(removed);
            self.measurements.clear();
            self.selector.clear();
            self.set_status(
                StatusKind::Info,
                "Debugging — running to the first breakpoint",
            );
        }
    }

    pub(super) fn finish_run(&mut self, result: RunFinished) {
        let RunEnd { stale, rerun } = self.execution.ended(result.id);
        self.editor.set_debug_line(None);

        if stale {
            let keys = result.objects.into_iter().flatten().map(|node| node.key);
            self.release(keys.collect());
            log::info!("Discarded the result of a run of the previous document");
            if rerun {
                self.start_run(false);
            }
            return;
        }

        match &result.error {
            Some(error) if result.interrupted => {
                // Not an error: the user asked for it.
                self.editor.error_line = None;
                let location = error
                    .line
                    .map(|line| format!(" at line {line}"))
                    .unwrap_or_default();
                self.set_status(StatusKind::Info, format!("Stopped{location}"));
                logging::push(Level::Info, "script", &error.text);
            }
            Some(error) => {
                self.editor.error_line = error.line;
                let location = error
                    .line
                    .map(|l| format!(" (line {l})"))
                    .unwrap_or_default();
                self.set_status(
                    StatusKind::Error,
                    format!("{}: {}{location}", error.type_name, error.message),
                );
                logging::push(Level::Error, "script", &error.text);
            }
            None => self.editor.error_line = None,
        }
        let ok = result.ok;
        self.traceback = result.error.filter(|_| !result.interrupted);

        if let Some(objects) = result.objects {
            let preserved = self
                .settings
                .objects
                .preserve_properties
                .then(|| self.scene.snapshot_properties());
            let was_empty = self.scene.is_empty();
            let decoding = Instant::now();
            // A scene that cannot be decoded is reported, and the rest of
            // the result (variables, a queued render) still applies.
            if let Some(mut nodes) = self.decode_scene_nodes(Some(result.id), objects) {
                self.attach_run_script(result.id, &mut nodes);
                if let Some(timings) = &result.timings {
                    log::info!("{}", timings.report(&result.receipt, decoding.elapsed()));
                }
                let count = nodes.len();
                if self.settings.objects.clear_before_run || result.debug {
                    let removed = self.scene.replace(nodes, preserved.as_ref());
                    self.release(removed);
                    self.measurements.clear();
                    self.selector.clear();
                } else {
                    self.scene.append(nodes, preserved.as_ref());
                }
                self.scene.overlay = Overlay::default();
                self.viewer.audio.sync_from_scene(&self.scene);
                if ok && !self.scene.is_empty() {
                    self.viewer.audio.play();
                }
                self.refresh_inspector();
                if self.settings.viewer.fit_automatically || was_empty {
                    let animate = self.settings.viewer.animate && !was_empty;
                    self.viewer.fit(&self.scene, animate, self.now());
                }
                if ok {
                    let plural = if count == 1 { "" } else { "s" };
                    self.set_status(
                        StatusKind::Success,
                        format!(
                            "Synthesized {count} track{plural} in {:.2} s",
                            result.duration
                        ),
                    );
                }
            }
        }
        if let Some(variables) = result.variables {
            self.variables = variables;
        }
        if result.stopped {
            self.set_status(StatusKind::Info, "Debugging stopped");
        }
        if result.cancelled {
            self.set_status(
                StatusKind::Info,
                "Meshing cancelled; the model shown before stays",
            );
        }
        if let Some(profile) = result.profile {
            self.show_profile(result.id, profile);
        }
        if ok {
            // The script may have started importing other modules.
            self.request_imports();
        }
        if rerun {
            self.start_run(false);
        }
    }

    /// A run of the worker failed before it could finish.
    pub(super) fn run_failed(&mut self, id: RequestId, message: String) {
        let RunEnd { stale, rerun } = self.execution.ended(id);
        self.editor.set_debug_line(None);
        if stale {
            log::warn!("A run of the previous document failed: {message}");
        } else {
            self.set_status(StatusKind::Error, message);
        }
        if rerun {
            self.start_run(false);
        }
    }

    pub(super) fn debug_command(&mut self, action: DebugAction) {
        if self.execution.resume().is_none() {
            return;
        }
        self.editor.sync_markers(&self.document.text);
        let breakpoints = self.editor.breakpoints.iter().copied().collect();
        self.worker.send(&Request::Debug {
            action,
            breakpoints,
        });
        self.editor.set_debug_line(None);
    }

    pub(super) fn stop_debugging(&mut self) {
        if self.is_paused() {
            self.debug_command(DebugAction::Stop);
        } else {
            self.stop();
        }
    }

    /// Aborts a running script by restarting the worker.
    /// Stops the running script or console command gently: Python raises
    /// `KeyboardInterrupt` in it, so the console and the caches survive.
    /// Python is restarted when Stop is pressed again, or when the script
    /// does not stop within `STOP_GRACE` (native code that never returns
    /// to Python). A paused debugging session ends through the debugger.
    pub(super) fn stop(&mut self) {
        if self.is_paused() {
            self.debug_command(DebugAction::Stop);
            return;
        }
        match self.execution.stop(Instant::now()) {
            StopPlan::Nothing => {}
            StopPlan::Interrupt(target) => {
                if self.worker.send(&Request::Interrupt { target }) {
                    self.set_status(StatusKind::Info, "Stopping… (Stop again to restart Python)");
                } else {
                    self.restart_to_stop("Execution stopped; Python was restarted");
                }
            }
            StopPlan::Restart => self.restart_to_stop("Execution stopped; Python was restarted"),
        }
    }

    /// Restarts Python once an interrupted script has had `STOP_GRACE` to
    /// stop, and wakes the UI for that moment.
    pub(super) fn poll_stop(&mut self, ctx: &egui::Context) {
        let Some(deadline) = self.execution.stop_deadline() else {
            return;
        };
        let now = Instant::now();
        if now >= deadline {
            self.restart_to_stop("The script did not stop; Python was restarted");
        } else {
            ctx.request_repaint_after(deadline - now);
        }
    }

    fn restart_to_stop(&mut self, message: &str) {
        self.start_worker();
        self.set_status(StatusKind::Warning, message);
    }

    /// Puts a profiled run's times in the gutter, on the lines of the
    /// document as it is now.
    fn show_profile(&mut self, id: RequestId, profile: RunProfile) {
        let Some(run_text) = self
            .run_script
            .as_ref()
            .filter(|(run, _)| *run == id)
            .map(|(_, script)| script.text.clone())
        else {
            return;
        };
        let mut lines: BTreeMap<usize, LineTiming> = profile
            .lines
            .iter()
            .map(|line| {
                let timing = LineTiming {
                    seconds: line.seconds,
                    hits: line.hits,
                    operations: Vec::new(),
                };
                (line.line, timing)
            })
            .collect();
        for operation in profile.operations {
            if let Some(timing) = lines.get_mut(&operation.line) {
                timing
                    .operations
                    .push((operation.name, operation.seconds, operation.calls));
            }
        }
        self.editor
            .set_line_profile(lines, &run_text, &self.document.text);
        // On the lines of the document as it is now.
        let slowest = self
            .editor
            .line_profile
            .iter()
            .max_by(|a, b| a.1.seconds.total_cmp(&b.1.seconds))
            .map(|(line, timing)| (*line, timing.seconds));
        if let Some((line, seconds)) = slowest {
            let text = format!(
                "Profiled: the slowest line is {line} ({}); times are in the gutter",
                format_seconds(seconds)
            );
            self.set_status(StatusKind::Success, text);
        }
    }

    /// Gives the nodes of a run the script text their lines refer to.
    pub(super) fn attach_run_script(&self, id: RequestId, nodes: &mut [SceneNode]) {
        if let Some((run, script)) = &self.run_script
            && *run == id
        {
            for node in nodes {
                node.attach_script(script);
            }
        }
    }

    /// Moves the editor to the script line that created an object, or one
    /// step of its construction stack in the CQ Inspector, following the
    /// edits made since the run.
    pub(super) fn go_to_source(&mut self, key: &str, step: Option<(usize, Option<usize>)>) {
        let Some(node) = self.scene.find(key) else {
            return;
        };
        let line = match step {
            None => node.line,
            Some(_) if self.inspector.key.as_deref() != Some(key) => None,
            Some((frame, item)) => {
                let frame = self.inspector.frames.get(frame);
                match item {
                    None => frame.and_then(|frame| frame.line),
                    Some(item) => frame
                        .and_then(|frame| frame.items.get(item))
                        .and_then(|item| item.line),
                }
            }
        };
        let (Some(line), Some(script)) = (line, node.script.clone()) else {
            self.set_status(
                StatusKind::Info,
                "No script line is known for this object (objects shown from the console have none)",
            );
            return;
        };
        if script.document != self.document_generation {
            self.set_status(
                StatusKind::Warning,
                "This object comes from another script; render again to go to its source",
            );
            return;
        }
        let Some(located) = text_ops::locate_line(&script.text, line, &self.document.text) else {
            self.set_status(
                StatusKind::Warning,
                format!("Line {line} was deleted since the last run"),
            );
            return;
        };
        self.editor.go_to_line(&self.document.text, located.line());
        match located {
            Located::Unchanged(at) => self.set_status(StatusKind::Info, format!("Line {at}")),
            Located::Moved(at) => self.set_status(
                StatusKind::Info,
                format!("Line {at} (line {line} when the script ran)"),
            ),
            Located::Changed(at) => self.set_status(
                StatusKind::Warning,
                format!("Line {at}: edited since the last run"),
            ),
        }
    }

    pub(super) fn decode_scene_nodes(
        &mut self,
        request: Option<RequestId>,
        objects: Vec<NodeMessage>,
    ) -> Option<Vec<SceneNode>> {
        match self.scene.decode_nodes(request, objects) {
            Ok(nodes) => Some(nodes),
            Err(error) => {
                log::error!("Cannot decode worker scene: {error}");
                self.set_status(StatusKind::Error, "Cannot update the 3D scene");
                None
            }
        }
    }
}
