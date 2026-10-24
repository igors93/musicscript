//! The open document: opening, saving, reloading after external changes,
//! crash-recovery checkpoints and local history.

use std::path::Path;
use std::time::Instant;

use egui::ViewportCommand;

use super::{AppState, StatusKind};
use crate::dialogs::{ExternalChange, ExternalChangeChoice, HistoryDialog, Intent, UnsavedChoice};
use crate::document::{Document, Encoding, is_not_utf8};
use crate::logging::{self, Level};
use crate::recovery::{CHECKPOINT_INTERVAL, RecoverySnapshot};
use crate::scene::Overlay;
use crate::worker::protocol::DebugAction;

impl AppState {
    pub(super) fn guard_unsaved(&mut self, intent: Intent) {
        if self.document.is_modified() {
            self.dialogs.unsaved = Some(intent);
        } else {
            self.perform(intent);
        }
    }

    pub fn resolve_unsaved(&mut self, intent: Intent, choice: UnsavedChoice) {
        match choice {
            UnsavedChoice::Save => {
                if self.save() {
                    self.perform(intent);
                }
            }
            UnsavedChoice::Discard => self.perform(intent),
            UnsavedChoice::Cancel => {}
        }
    }

    pub fn resolve_recovery_session(&mut self, snapshot: RecoverySnapshot, restore: bool) {
        if let Err(error) = self.recovery.clear_session() {
            self.dialogs.error(
                "Cannot update recovery state",
                format!("Your recovery was kept on disk.\n\n{error}"),
            );
            self.dialogs.recovery_session = Some(snapshot);
            return;
        }
        if restore {
            let name = snapshot.display_name();
            self.set_document(snapshot.into_document(), false);
            self.checkpoint_recovery();
            self.set_status(
                StatusKind::Success,
                format!("Recovered unsaved work for {name}"),
            );
        } else {
            self.set_status(
                StatusKind::Info,
                "Discarded the interrupted recovery session",
            );
        }
    }

    pub fn restore_history(&mut self, snapshot: RecoverySnapshot) {
        self.checkpoint_recovery();
        let name = snapshot.display_name();
        // Another version of the same file, like a reload: the model stays.
        self.set_document(snapshot.into_document(), true);
        self.checkpoint_recovery();
        self.set_status(
            StatusKind::Success,
            format!("Restored a local history version of {name}"),
        );
    }

    pub fn resolve_external_change(
        &mut self,
        change: ExternalChange,
        choice: ExternalChangeChoice,
    ) {
        if self.document.path() != Some(change.path.as_path()) {
            return;
        }
        match choice {
            ExternalChangeChoice::KeepLocal => {
                self.set_status(
                    StatusKind::Info,
                    format!("Kept unsaved changes to {}", self.document.display_name()),
                );
                self.rerun_after_external_change(change.rerun_after_resolution);
            }
            ExternalChangeChoice::Reload => self.reload_external_change(change),
            ExternalChangeChoice::SaveCopy => self.save_copy_then_reload(change),
        }
    }

    fn perform(&mut self, intent: Intent) {
        match intent {
            Intent::New => self.set_document(Document::untitled(""), false),
            Intent::Open => {
                let mut dialog = rfd::FileDialog::new().add_filter("Python script", &["py"]);
                if let Some(dir) = self.document.path().and_then(Path::parent) {
                    dialog = dialog.set_directory(dir);
                }
                if let Some(path) = dialog.pick_file() {
                    self.open_path(&path);
                }
            }
            Intent::OpenPath(path) => self.open_path(&path),
            Intent::OpenExample(index) => {
                if let Some(example) = self.examples.get(index) {
                    let document = Document::example(&example.title, example.source);
                    self.set_document(document, false);
                    self.set_status(
                        StatusKind::Info,
                        "Examples are read-only; saving asks for a new location",
                    );
                }
            }
            Intent::Quit => {
                self.clear_recovery_session();
                self.allow_close = true;
                self.ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    fn open_path(&mut self, path: &Path) {
        self.open_path_with(path, Encoding::Utf8);
    }

    /// Opens a file that is not UTF-8 after the user agreed to read it as
    /// `encoding` (and to convert it on save).
    pub fn open_path_with(&mut self, path: &Path, encoding: Encoding) {
        match Document::open_with(path, encoding) {
            Ok(document) => {
                self.set_document(document, false);
                self.settings.add_recent_file(path);
                self.settings.last_file = Some(path.to_path_buf());
                let status = match encoding {
                    Encoding::Utf8 => format!("Opened {}", path.display()),
                    _ => format!(
                        "Opened {} as {}; saving converts it to UTF-8",
                        path.display(),
                        encoding.label()
                    ),
                };
                self.set_status(StatusKind::Info, status);
            }
            Err(err) if is_not_utf8(&err) => {
                self.dialogs.legacy_encoding = Some(path.to_path_buf());
            }
            Err(err) => self
                .dialogs
                .error("Cannot open file", format!("{}\n\n{err}", path.display())),
        }
    }

    /// Replaces the document. Unless `keep_scene` (another version of the
    /// same file), the model, its selection and the variables of the
    /// previous file are cleared, and a run still going for it is discarded:
    /// its result would look like the output of the new script.
    fn set_document(&mut self, document: Document, keep_scene: bool) {
        self.clear_recovery_session();
        self.remember_breakpoints();
        self.remember_views();
        self.document = document;
        self.dialogs.external_change = None;
        self.dialogs.history = None;
        self.next_recovery = Instant::now() + CHECKPOINT_INTERVAL;
        self.editor.reset();
        self.restore_breakpoints();
        self.restore_views();
        self.traceback = None;
        if keep_scene {
            return;
        }
        self.document_generation += 1;
        if self.execution.abandon() {
            self.debug_command(DebugAction::Stop);
        }
        let removed = self.scene.clear();
        self.release(removed);
        self.scene.overlay = Overlay::default();
        self.measurements.clear();
        self.selector.clear();
        self.inspector.clear();
        self.variables.clear();
    }

    /// Returns whether the document ended up saved.
    pub(super) fn save(&mut self) -> bool {
        self.checkpoint_recovery();
        match self.document.save() {
            Ok(true) => {
                self.after_save();
                true
            }
            Ok(false) => {
                if self.document.example_title().is_some() {
                    self.set_status(
                        StatusKind::Info,
                        "Examples are read-only — choose a new location to save your changes",
                    );
                }
                self.save_as()
            }
            Err(err) => {
                self.dialogs.error("Cannot save", err.to_string());
                false
            }
        }
    }

    pub(super) fn save_as(&mut self) -> bool {
        self.checkpoint_recovery();
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Python script", &["py"])
            .set_file_name(self.document.path().and_then(Path::file_name).map_or_else(
                || "script.py".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            ));
        if let Some(dir) = self.document.path().and_then(Path::parent) {
            dialog = dialog.set_directory(dir);
        }
        let Some(mut path) = dialog.save_file() else {
            return false;
        };
        if path.extension().is_none() {
            path.set_extension("py");
        }
        if let Err(error) = self.recovery.archive_path(&path) {
            log::warn!("Cannot archive the previous Save As target: {error}");
        }
        match self.document.save_as(&path) {
            Ok(()) => {
                self.after_save();
                true
            }
            Err(err) => {
                self.dialogs
                    .error("Cannot save", format!("{}\n\n{err}", path.display()));
                false
            }
        }
    }

    fn save_copy_then_reload(&mut self, change: ExternalChange) {
        let stem = change
            .path
            .file_stem()
            .map_or_else(|| "script".into(), |name| name.to_string_lossy());
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Python script", &["py"])
            .set_file_name(format!("{stem}.local-copy.py"));
        if let Some(directory) = change.path.parent() {
            dialog = dialog.set_directory(directory);
        }
        let Some(mut path) = dialog.save_file() else {
            self.dialogs.external_change = Some(change);
            return;
        };
        if path.extension().is_none() {
            path.set_extension("py");
        }
        match self.document.save_copy(&path) {
            Ok(()) => self.reload_external_change(change),
            Err(error) => {
                self.dialogs.error(
                    "Cannot save a copy",
                    format!("{}\n\n{error}", path.display()),
                );
                self.dialogs.external_change = Some(change);
            }
        }
    }

    fn reload_external_change(&mut self, change: ExternalChange) {
        self.checkpoint_recovery();
        let backup = if self.document.is_modified() {
            match self
                .document
                .create_recovery_backup(&self.cache_dir.join("recovery"))
            {
                Ok(path) => Some(path),
                Err(error) => {
                    self.dialogs.error(
                        "Cannot create recovery backup",
                        format!(
                            "The file was not reloaded and your edits are unchanged.\n\n{error}"
                        ),
                    );
                    self.dialogs.external_change = Some(change);
                    return;
                }
            }
        } else {
            None
        };

        match self.document.apply_disk_version(change.disk) {
            Ok(_) => {
                if let Some(path) = &backup {
                    logging::push(
                        Level::Info,
                        "recovery",
                        &format!("Saved unsaved edits to {}", path.display()),
                    );
                }
                let message = backup.map_or_else(
                    || format!("Reloaded {}", self.document.display_name()),
                    |path| {
                        format!(
                            "Reloaded {}; recovery backup: {}",
                            self.document.display_name(),
                            path.display()
                        )
                    },
                );
                self.set_status(StatusKind::Info, message);
                self.clear_recovery_session();
                self.rerun_after_external_change(change.rerun_after_resolution);
            }
            Err(error) => self
                .dialogs
                .error("Cannot reload the script", error.to_string()),
        }
    }

    fn rerun_after_external_change(&mut self, rerun: bool) {
        if rerun {
            self.start_run(false);
        }
    }

    /// Shows the breakpoints the document had when it was last open.
    pub(super) fn restore_breakpoints(&mut self) {
        if let Some(path) = self.document.path() {
            let saved = self.settings.breakpoints_of(path).to_vec();
            self.editor.restore_breakpoints(&self.document.text, &saved);
        }
    }

    /// Keeps the breakpoints of the document for the next time it opens.
    pub(super) fn remember_breakpoints(&mut self) {
        if let Some(path) = self.document.path().map(Path::to_path_buf) {
            let saved = self.editor.saved_breakpoints(&self.document.text);
            self.settings.remember_breakpoints(&path, saved);
        }
    }

    /// Shows the named views the document had when it was last open.
    pub(super) fn restore_views(&mut self) {
        self.viewer.named_views = self
            .document
            .path()
            .map(|path| self.settings.views_of(path).to_vec())
            .unwrap_or_default();
    }

    /// Keeps the named views of the document for the next time it opens.
    pub(super) fn remember_views(&mut self) {
        if let Some(path) = self.document.path().map(Path::to_path_buf) {
            let views = self.viewer.named_views.clone();
            self.settings.remember_views(&path, views);
        }
    }

    fn after_save(&mut self) {
        // Under the new name after a Save As.
        self.remember_breakpoints();
        self.remember_views();
        if let Err(error) = self.recovery.archive(&self.document) {
            log::warn!("Cannot add the saved document to local history: {error}");
        }
        self.clear_recovery_session();
        if let Some(path) = self.document.path().map(Path::to_path_buf) {
            self.settings.add_recent_file(&path);
            self.settings.last_file = Some(path.clone());
            self.set_status(StatusKind::Success, format!("Saved {}", path.display()));
        }
        // The saved script may import other modules now.
        self.request_imports();
    }

    pub(super) fn show_local_history(&mut self) {
        self.checkpoint_recovery();
        match self.recovery.history(self.document.path()) {
            Ok(versions) if versions.is_empty() => self.dialogs.info(
                "Local history",
                "No recovery versions are available for this document yet.",
            ),
            Ok(versions) => {
                self.dialogs.history = Some(HistoryDialog {
                    versions,
                    selected: 0,
                });
            }
            Err(error) => self
                .dialogs
                .error("Cannot read local history", error.to_string()),
        }
    }

    pub(super) fn checkpoint_recovery(&mut self) {
        // The startup document is a separate clean buffer while the recovered
        // session awaits a decision. It must never clear that pending session.
        // When quitting was confirmed, the edits were saved or deliberately
        // discarded ("Don't save"); eframe's final `save` must not bring
        // them back as an interrupted session on the next start.
        if self.dialogs.recovery_session.is_some() || self.allow_close {
            return;
        }
        match self.recovery.checkpoint(&self.document) {
            Ok(_) => self.recovery_error_reported = false,
            Err(error) => {
                log::warn!("Cannot create recovery checkpoint: {error}");
                if !self.recovery_error_reported {
                    self.set_status(
                        StatusKind::Warning,
                        "Automatic recovery is unavailable; see the Log panel",
                    );
                    self.recovery_error_reported = true;
                }
            }
        }
    }

    pub(super) fn clear_recovery_session(&mut self) {
        if let Err(error) = self.recovery.clear_session() {
            log::warn!("Cannot clear recovery session: {error}");
        }
    }

    pub(super) fn poll_recovery(&mut self, ctx: &egui::Context) {
        if self.dialogs.recovery_session.is_some() {
            return;
        }
        if !self.document.is_modified() {
            self.clear_recovery_session();
            self.next_recovery = Instant::now() + CHECKPOINT_INTERVAL;
            return;
        }

        let now = Instant::now();
        if now >= self.next_recovery {
            self.checkpoint_recovery();
            self.next_recovery = now + CHECKPOINT_INTERVAL;
        }
        ctx.request_repaint_after(self.next_recovery.saturating_duration_since(now));
    }
}
