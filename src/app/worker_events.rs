//! The worker process: starting it, recovering from its crashes, sending
//! requests and routing its messages to whatever asked for them.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use super::execution::Pending;
use super::{AppState, StatusKind};
use crate::embedded;
use crate::logging::{self, Level};
use crate::panels::EntryKind;
use crate::scene::{Grid, Mesh, NodeKey, Overlay};
use crate::updates;
use crate::worker::protocol::{DebugAction, Message, OutputStream, Request, RequestId};
use crate::worker::{self, Event, LaunchConfig, launch};

impl AppState {
    /// Asks for CQ-editor's newer releases once a day, without a word
    /// unless one is out (Preferences > General turns it off).
    fn check_updates_daily(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        if !updates::check_due(&self.settings.general, now) || !self.worker.is_ready() {
            return;
        }
        let sent = self.send_tracked(Pending::Updates { automatic: true }, |id| {
            Request::CheckUpdates {
                id,
                editor: updates::VERSION.to_owned(),
                cadquery: false,
            }
        });
        if sent {
            self.settings.general.last_update_check = now;
        }
    }

    /// The interpreter Python is (or will be) started with: the command
    /// line's, else the preferences', else the one detected.
    pub(super) fn python_in_use(&self) -> std::path::PathBuf {
        let explicit = self
            .python_override
            .clone()
            .unwrap_or_else(|| self.settings.general.python.clone());
        launch::find_python(Some(&explicit))
    }

    pub(super) fn start_worker(&mut self) {
        let python = self.python_in_use();
        let worker_dir = match embedded::worker_dir(&self.cache_dir) {
            Ok(dir) => dir,
            Err(err) => {
                self.set_status(
                    StatusKind::Error,
                    format!("Cannot prepare the Python worker: {err}"),
                );
                return;
            }
        };
        self.worker.start(LaunchConfig { python, worker_dir });
        self.worker_was_ready = false;
        // Look for interpreters again if this one fails too: one may have
        // been installed meanwhile.
        self.dialogs.python_candidates = None;
        self.reset_execution_state();
        // The new process knows nothing about the old one's inspections.
        self.inspector.clear();
        self.scene.clear_topology_selection();
        self.measurements.clear();
        self.selector.clear();
        self.scene.overlay = Overlay::default();
    }

    /// Forgets everything that was in flight in the worker.
    fn reset_execution_state(&mut self) {
        self.execution.reset();
        self.console.busy = false;
        self.editor.set_debug_line(None);
        self.inspector.loading = false;
        if let Some(upload) = &mut self.dialogs.upload {
            upload.busy = false;
        }
    }

    /// Sends a request with a fresh id, remembering what it was for.
    pub(super) fn send_tracked(
        &mut self,
        kind: Pending,
        build: impl FnOnce(RequestId) -> Request,
    ) -> bool {
        if !self.worker.is_ready() {
            self.set_status(StatusKind::Warning, "Python is not ready yet");
            return false;
        }
        let id = self.worker.next_id();
        if self.worker.send(&build(id)) {
            self.execution.track(id, kind);
            true
        } else {
            false
        }
    }

    pub(super) fn release(&mut self, keys: Vec<NodeKey>) {
        if !keys.is_empty() {
            self.worker.send(&Request::Release { keys });
        }
    }

    pub(super) fn process_worker_events(&mut self) {
        for event in self.worker.poll() {
            match event {
                Event::Message(message) => self.handle_message(message),
                Event::Stderr(line) => logging::push(Level::Warn, "python", &line),
                Event::Exited => {
                    let reason = match self.worker.status() {
                        worker::Status::Failed(reason) => reason.clone(),
                        _ => "The Python worker stopped".to_owned(),
                    };
                    log::error!("{reason}");
                    let was_busy = self.is_busy();
                    self.reset_execution_state();
                    if self.worker_was_ready {
                        // A crash inside OpenCascade must not leave the
                        // editor without Python: start a fresh worker.
                        self.set_status(
                            StatusKind::Error,
                            if was_busy {
                                "Python crashed while running the script; it was restarted"
                            } else {
                                "Python stopped unexpectedly; it was restarted"
                            },
                        );
                        self.start_worker();
                    } else {
                        self.set_status(StatusKind::Error, reason);
                    }
                }
            }
        }
        // References in a response are only valid while its request runs.
        let execution = &self.execution;
        self.scene.release_geometry(|id| execution.is_pending(id));
    }

    pub(super) fn handle_message(&mut self, message: Message) {
        match message {
            Message::Ready(info) => {
                log::info!(
                    "Python {} with CadQuery {} / OCP {} is ready ({})",
                    info.python,
                    info.cadquery,
                    info.ocp,
                    info.executable
                );
                if !info.completion {
                    log::info!("Install jedi in the Python environment for code completion");
                }
                self.worker_was_ready = true;
                self.check_updates_daily();
                self.set_status(
                    StatusKind::Success,
                    format!("MusicScript {} ready", info.cadquery),
                );
                if let Some(debug) = self.execution.worker_ready() {
                    self.start_run(debug);
                }
            }
            Message::Fatal { message, detail } => {
                log::error!("{message}");
                if let Some(detail) = detail {
                    logging::push(Level::Error, "python", &detail);
                }
                self.set_status(StatusKind::Error, message);
            }
            Message::Output { id, stream, text } => {
                let to_console = id
                    .is_some_and(|id| matches!(self.execution.pending(id), Some(Pending::Console)));
                match (to_console, stream) {
                    (true, OutputStream::Stdout) => self.console.push(EntryKind::Output, &text),
                    (true, OutputStream::Stderr) => self.console.push(EntryKind::Error, &text),
                    (false, OutputStream::Stdout) => logging::push(Level::Info, "print", &text),
                    (false, OutputStream::Stderr) => logging::push(Level::Warn, "stderr", &text),
                }
            }
            Message::Log {
                level,
                source,
                message,
            } => {
                logging::push(Level::from_python(&level), &source, &message);
            }
            Message::RunFinished(result) => self.finish_run(*result),
            Message::Progress {
                id,
                stage,
                done,
                total,
            } => {
                if stage == "meshing" {
                    self.execution
                        .meshing_progress(id, done, total, std::time::Instant::now());
                }
            }
            Message::DebugPaused {
                id,
                line,
                variables,
                objects,
            } => {
                if self.execution.paused(id) {
                    // A session of the previous document reached a breakpoint.
                    self.release(objects.into_iter().map(|node| node.key).collect());
                    self.debug_command(DebugAction::Stop);
                    return;
                }
                self.editor.set_debug_line(Some(line));
                self.variables = variables;
                let Some(mut nodes) = self.decode_scene_nodes(Some(id), objects) else {
                    return;
                };
                self.attach_run_script(id, &mut nodes);
                let removed = self.scene.replace(nodes, None);
                self.release(removed);
                self.viewer.audio.sync_from_scene(&self.scene);
                self.measurements.clear();
                self.selector.clear();
                self.refresh_inspector();
                self.set_status(StatusKind::Info, format!("Paused at line {line}"));
            }
            Message::Completions {
                id,
                available,
                items,
                signatures,
            } => {
                match self.execution.answer(id) {
                    Some(Pending::ConsoleCompletion) => {
                        let inserts: Vec<(String, String)> =
                            items.into_iter().map(|i| (i.name, i.insert)).collect();
                        self.console.complete(&inserts, &signatures);
                    }
                    // Offered while typing a name: names only, and silence
                    // when there are none.
                    Some(Pending::Completion { automatic: true }) => {
                        if !items.is_empty() {
                            self.editor.show_completions(items, Vec::new());
                        }
                    }
                    _ if !available => self.set_status(
                        StatusKind::Info,
                        "Code completion needs jedi: pip install jedi",
                    ),
                    _ => {
                        if !self.editor.show_completions(items, signatures) {
                            self.set_status(StatusKind::Info, "No completions available");
                        }
                    }
                }
            }
            Message::Description { id, documentation } => {
                if matches!(self.execution.answer(id), Some(Pending::Describe)) {
                    self.editor.show_documentation(documentation);
                }
            }
            Message::Inspection { id, key, frames } => {
                self.execution.answer(id);
                if self.inspector.key.as_deref() == Some(key.as_str()) {
                    self.inspector.frames = frames;
                    self.inspector.loading = false;
                }
            }
            Message::InspectionGeometry { id, meshes, plane } => {
                // Only for the entry still selected: the inspector may show
                // another workplane, or another step, by now.
                let current = match self.execution.answer(id) {
                    Some(Pending::InspectGeometry { key, entry }) => {
                        self.inspector.key.as_ref() == Some(&key)
                            && self.inspector.selected == Some(entry)
                    }
                    _ => false,
                };
                if current && self.inspector.show_in_view {
                    self.scene.overlay = Overlay {
                        meshes: meshes
                            .into_iter()
                            .map(|m| Arc::new(Mesh::from(m)))
                            .collect(),
                        grid: plane.map(|p| Grid {
                            origin: p.origin.into(),
                            x_dir: p.x_dir.into(),
                            z_dir: p.z_dir.into(),
                            size: p.size,
                        }),
                    };
                }
            }
            Message::Measurement(result) => {
                let id = result.id;
                let tracked = matches!(self.execution.answer(id), Some(Pending::Measure));
                if tracked && self.measurements.request_id == Some(id) {
                    self.measurements.loading = false;
                    self.measurements.error = None;
                    self.measurements.request_id = None;
                    self.measurements.result = Some(*result);
                }
            }
            Message::MeasurementCode {
                id,
                code,
                verified,
                problem,
            } => {
                if matches!(self.execution.answer(id), Some(Pending::MeasurementCode)) {
                    self.measurement_code_arrived(code, verified, problem);
                }
            }
            Message::Selector(reply) => {
                let id = reply.id;
                let tracked = matches!(self.execution.answer(id), Some(Pending::Selector));
                // Only the reply for the current selection.
                if tracked && self.selector.request_id == Some(id) {
                    self.selector.loading = false;
                    self.selector.request_id = None;
                    self.selector.chosen = 0;
                    self.selector.reply = Some(reply);
                    if let Some(usage) = self.selector.then.take()
                        && let Some(candidate) = self.selector.candidate().cloned()
                    {
                        self.apply_selector(usage, &candidate);
                    }
                }
            }
            Message::SceneAdd { id, objects } => {
                let was_empty = self.scene.is_empty();
                let Some(nodes) = self.decode_scene_nodes(id, objects) else {
                    return;
                };
                self.scene.append(nodes, None);
                self.viewer.audio.sync_from_scene(&self.scene);
                if was_empty {
                    self.viewer.fit(&self.scene, false, self.now());
                }
            }
            Message::Display(output) => {
                if matches!(self.execution.pending(output.id), Some(Pending::Console)) {
                    self.console.push_display(output);
                }
            }
            Message::ConsoleResult { id, more } => {
                self.execution.answer(id);
                self.console.more = more;
                self.console.busy = false;
            }
            Message::ExportFinished { id, path } => {
                self.execution.answer(id);
                self.set_status(StatusKind::Success, format!("Exported {path}"));
            }
            Message::UploadFinished { id, status, body } => {
                self.execution.answer(id);
                if let Some(upload) = &mut self.dialogs.upload {
                    upload.finished(status, &body);
                }
            }
            Message::Updates {
                id,
                current,
                newer,
                editor,
            } => {
                let automatic = matches!(
                    self.execution.answer(id),
                    Some(Pending::Updates { automatic: true })
                );
                if let Some(editor) = &editor {
                    self.settings.general.newer_release = editor.newer.first().cloned();
                }
                let available = updates::available(&self.settings.general).cloned();
                if automatic {
                    if let Some(release) = available {
                        self.set_status(
                            StatusKind::Info,
                            format!(
                                "CQ-editor {} is available: Help > Download CQ-editor {}",
                                release.tag, release.tag
                            ),
                        );
                    }
                    return;
                }
                self.status = None;
                let editor_text = match &available {
                    Some(release) => format!(
                        "CQ-editor {} is available (you have {}): Help > Download \
                         CQ-editor {} opens its page.",
                        release.tag,
                        updates::VERSION,
                        release.tag
                    ),
                    None => format!("CQ-editor {} is the latest version.", updates::VERSION),
                };
                let cadquery_text = if newer.is_empty() {
                    format!("CadQuery {current} is the latest version.")
                } else {
                    format!(
                        "Newer CadQuery releases ({current} installed):\n{}",
                        newer.join("\n")
                    )
                };
                let title = if available.is_some() || !newer.is_empty() {
                    "Updates available"
                } else {
                    "No updates available"
                };
                self.dialogs
                    .info(title, format!("{editor_text}\n\n{cadquery_text}"));
            }
            Message::Imports { id, paths } => {
                let Some(Pending::Imports(script)) = self.execution.answer(id) else {
                    return;
                };
                // Answers for another file, or after watching changed, are
                // outdated.
                if self.watched_imports_of().as_deref() != Some(script.as_path()) {
                    return;
                }
                // The script and its current imports: modules no longer
                // imported stop being watched, and none is watched twice.
                let files: BTreeSet<PathBuf> = paths
                    .into_iter()
                    .map(PathBuf::from)
                    .chain([script])
                    .collect();
                self.watcher.set_files(files.into_iter().collect());
            }
            Message::Error { id, message } => self.request_failed(id, message),
        }
    }

    pub(super) fn request_failed(&mut self, id: Option<RequestId>, message: String) {
        let kind = id.and_then(|id| self.execution.answer(id).map(|kind| (id, kind)));
        match kind {
            Some((_, Pending::Console)) => {
                self.console.push(EntryKind::Error, &format!("{message}\n"));
                self.console.busy = false;
            }
            Some((_, Pending::Export)) => self.dialogs.error("Export failed", message),
            Some((_, Pending::Upload)) => {
                if let Some(upload) = &mut self.dialogs.upload {
                    upload.failed(&message);
                }
            }
            Some((_, Pending::Updates { automatic: true })) => {
                // Offline, rate-limited…: tried again tomorrow, silently.
                log::info!("Cannot check for CQ-editor updates: {message}");
            }
            Some((_, Pending::Updates { automatic: false })) => {
                self.status = None;
                self.dialogs.error("Cannot check for updates", message);
            }
            Some((_, Pending::Inspect | Pending::InspectGeometry { .. })) => {
                self.inspector.loading = false;
                self.set_status(StatusKind::Warning, message);
            }
            Some((id, Pending::Run)) => self.run_failed(id, message),
            Some((id, Pending::Measure)) if self.measurements.request_id == Some(id) => {
                self.measurements.loading = false;
                self.measurements.error = Some(message);
                self.measurements.request_id = None;
            }
            Some((_, Pending::Measure)) => {}
            Some((id, Pending::Selector)) if self.selector.request_id == Some(id) => {
                self.selector.loading = false;
                self.selector.request_id = None;
                if self.selector.then.take().is_some() {
                    self.set_status(StatusKind::Warning, message.clone());
                }
                self.selector.error = Some(message);
            }
            Some((_, Pending::Selector)) => {}
            Some((_, Pending::MeasurementCode)) => {
                self.set_status(
                    StatusKind::Warning,
                    format!("Cannot write the measurement as code: {message}"),
                );
            }
            // Hovering over a name the worker cannot describe is not news.
            Some((_, Pending::Describe | Pending::Completion { automatic: true })) => {
                log::debug!("{message}");
            }
            Some((
                _,
                Pending::Completion { .. } | Pending::ConsoleCompletion | Pending::Imports(_),
            ))
            | None => {
                log::warn!("{message}");
                self.set_status(StatusKind::Warning, message);
            }
        }
    }
}
