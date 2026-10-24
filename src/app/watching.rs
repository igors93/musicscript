//! Automatic reload: the script and the local modules it imports are
//! watched, and a change reloads the script and renders it again.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::execution::Pending;
use super::{AppState, StatusKind};
use crate::dialogs::ExternalChange;
use crate::document::ExternalUpdate;
use crate::worker::protocol::Request;

impl AppState {
    pub(super) fn sync_watcher(&mut self) {
        let key = (
            self.settings.editor.autoreload,
            self.settings.editor.watch_imported_modules,
            self.document.path().map(Path::to_path_buf),
        );
        if self.watch_key.as_ref() == Some(&key) {
            return;
        }
        let (autoreload, _, path) = key.clone();
        self.watch_key = Some(key);
        match path.filter(|_| autoreload) {
            Some(path) => {
                self.watcher.set_files(vec![path]);
                self.request_imports();
            }
            None => self.watcher.clear(),
        }
    }

    /// The script whose local imports are watched, if any.
    pub(super) fn watched_imports_of(&self) -> Option<PathBuf> {
        let editor = &self.settings.editor;
        (editor.autoreload && editor.watch_imported_modules)
            .then(|| self.document.path().map(Path::to_path_buf))
            .flatten()
    }

    /// Asks which local modules the script imports, so the watcher follows
    /// them. Called whenever watching starts and again after each save and
    /// successful run, as the imports change with the script.
    pub(super) fn request_imports(&mut self) {
        let Some(script) = self.watched_imports_of() else {
            return;
        };
        if !self.worker.is_ready() {
            return; // asked again after the first run
        }
        let path = script.display().to_string();
        self.send_tracked(Pending::Imports(script), |id| Request::Imports { id, path });
    }

    pub(super) fn poll_watcher(&mut self, ctx: &egui::Context) {
        if !self.watcher.is_watching() {
            return;
        }
        ctx.request_repaint_after(self.watcher.next_poll_in());
        let delay = Duration::from_millis(self.settings.editor.autoreload_delay_ms);
        let changed = self.watcher.poll(delay);
        if changed.is_empty() {
            return;
        }

        let document_path = self.document.path().map(Path::to_path_buf);
        let document_changed = document_path
            .as_ref()
            .is_some_and(|path| changed.iter().any(|changed| changed == path));
        let imported_changed = changed
            .iter()
            .any(|changed| Some(changed) != document_path.as_ref());

        if !document_changed {
            if let Some(conflict) = &mut self.dialogs.external_change {
                conflict.rerun_after_resolution |= imported_changed;
            } else if imported_changed {
                self.start_run(false);
            }
            return;
        }

        let version = match self.document.disk_version() {
            Ok(Some(version)) => version,
            Ok(None) => return,
            Err(error) => {
                log::warn!("Cannot read the changed script: {error}");
                self.dialogs
                    .error("Cannot read the changed script", error.to_string());
                return;
            }
        };

        match self.document.classify_external_update(&version) {
            // A timestamp-only rewrite of the main file must neither erase
            // local edits nor trigger a needless rebuild.
            ExternalUpdate::Unchanged => {
                if imported_changed {
                    self.start_run(false);
                }
            }
            ExternalUpdate::Conflict => {
                self.dialogs.external_change = Some(ExternalChange {
                    path: document_path.expect("a watched document has a path"),
                    disk: version,
                    show_comparison: false,
                    rerun_after_resolution: true,
                });
                self.set_status(
                    StatusKind::Warning,
                    "File changed on disk; waiting for your decision",
                );
            }
            ExternalUpdate::MatchesBuffer => match self.document.apply_disk_version(version) {
                Ok(_) => {
                    self.set_status(
                        StatusKind::Info,
                        format!("Reloaded {}", self.document.display_name()),
                    );
                    self.clear_recovery_session();
                    self.start_run(false);
                }
                Err(error) => log::warn!("Cannot reload the script: {error}"),
            },
            ExternalUpdate::Reload => match self.document.reload(version) {
                Ok(_) => {
                    self.set_status(
                        StatusKind::Info,
                        format!("Reloaded {}", self.document.display_name()),
                    );
                    self.start_run(false);
                }
                Err(error) => log::warn!("Cannot reload the script: {error}"),
            },
        }
    }
}
