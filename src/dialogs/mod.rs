//! Modal and floating windows: preferences, upload, about, unsaved changes
//! and messages.

pub mod capture;
pub mod environment;
pub mod preferences;
pub mod upload;

use std::path::{Path, PathBuf};

use egui::{Align2, RichText};
use egui_phosphor::regular as icon;

use crate::app::AppState;
use crate::commands::Action;
use crate::document::{DiskVersion, Encoding};
use crate::recovery::RecoverySnapshot;
use crate::worker::Problem;

/// Something that would discard unsaved changes, waiting for confirmation.
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    New,
    Open,
    OpenPath(PathBuf),
    OpenExample(usize),
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsavedChoice {
    Save,
    Discard,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalChangeChoice {
    Reload,
    KeepLocal,
    SaveCopy,
}

/// An on-disk edit that conflicts with unsaved editor contents.
pub struct ExternalChange {
    pub path: PathBuf,
    pub disk: DiskVersion,
    pub show_comparison: bool,
    pub rerun_after_resolution: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecoverySessionChoice {
    Restore,
    Discard,
}

pub struct HistoryDialog {
    pub versions: Vec<RecoverySnapshot>,
    pub selected: usize,
}

enum HistoryChoice {
    Restore(RecoverySnapshot),
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    Info,
    Error,
}

pub struct Message {
    pub title: String,
    pub text: String,
    pub kind: MessageKind,
}

#[derive(Default)]
pub struct Dialogs {
    pub preferences: bool,
    pub preferences_page: preferences::Page,
    pub about: bool,
    pub upload: Option<upload::UploadDialog>,
    pub capture: capture::CaptureDialog,
    pub environment: Option<environment::EnvironmentDialog>,
    pub unsaved: Option<Intent>,
    pub external_change: Option<ExternalChange>,
    pub recovery_session: Option<RecoverySnapshot>,
    pub history: Option<HistoryDialog>,
    /// A file that is not UTF-8, waiting for permission to read it as
    /// Windows-1252.
    pub legacy_encoding: Option<PathBuf>,
    pub message: Option<Message>,
    /// Interpreters found on this computer, looked for once Python failed.
    pub python_candidates: Option<Vec<PathBuf>>,
}

impl Dialogs {
    pub fn info(&mut self, title: &str, text: impl Into<String>) {
        self.message = Some(Message {
            title: title.to_owned(),
            text: text.into(),
            kind: MessageKind::Info,
        });
    }

    pub fn error(&mut self, title: &str, text: impl Into<String>) {
        self.message = Some(Message {
            title: title.to_owned(),
            text: text.into(),
            kind: MessageKind::Error,
        });
    }
}

pub fn show(ctx: &egui::Context, state: &mut AppState) {
    let mut actions = Vec::new();

    if state.dialogs.preferences {
        let dialogs = &mut state.dialogs;
        preferences::window(
            ctx,
            &mut dialogs.preferences,
            &mut dialogs.preferences_page,
            &mut state.settings,
            &mut actions,
        );
    }

    if let Some(upload) = &mut state.dialogs.upload
        && !upload.window(ctx, &mut actions)
    {
        state.dialogs.upload = None;
    }

    if let Some(dialog) = &mut state.dialogs.environment
        && !dialog.window(ctx, &mut actions)
    {
        state.dialogs.environment = None;
    }

    if state.dialogs.capture.open {
        capture::window(
            ctx,
            &mut state.dialogs.capture,
            state.viewer.view_px(),
            state.viewer.turntable_progress(),
            &mut actions,
        );
    }

    if state.dialogs.about {
        about(ctx, state);
    }

    let recovery_was_open = state.dialogs.recovery_session.is_some();
    let recovery_choice = state
        .dialogs
        .recovery_session
        .as_ref()
        .and_then(|snapshot| recovery_session(ctx, snapshot));
    if let Some(choice) = recovery_choice
        && let Some(snapshot) = state.dialogs.recovery_session.take()
    {
        state.resolve_recovery_session(snapshot, choice == RecoverySessionChoice::Restore);
    }
    if recovery_was_open {
        state.queue(actions);
        return;
    }

    if let Some(intent) = state.dialogs.unsaved.clone()
        && let Some(choice) = unsaved_changes(ctx, &state.document.display_name())
    {
        state.dialogs.unsaved = None;
        state.resolve_unsaved(intent, choice);
    }

    if let Some(path) = state.dialogs.legacy_encoding.clone()
        && let Some(open) = legacy_encoding(ctx, &path)
    {
        state.dialogs.legacy_encoding = None;
        if open {
            state.open_path_with(&path, Encoding::Windows1252);
        }
    }

    let document_name = state.document.display_name();
    let external_choice = state
        .dialogs
        .external_change
        .as_mut()
        .and_then(|change| external_change(ctx, &document_name, &state.document.text, change));
    if let Some(choice) = external_choice
        && let Some(change) = state.dialogs.external_change.take()
    {
        state.resolve_external_change(change, choice);
    }

    let history_choice = state
        .dialogs
        .history
        .as_mut()
        .and_then(|history| local_history(ctx, history));
    match history_choice {
        Some(HistoryChoice::Restore(snapshot)) => {
            state.dialogs.history = None;
            state.restore_history(snapshot);
        }
        Some(HistoryChoice::Close) => state.dialogs.history = None,
        None => {}
    }

    if let Some(message) = &state.dialogs.message
        && message_box(ctx, message)
    {
        state.dialogs.message = None;
    }

    state.queue(actions);
}

fn modal_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::popup(&ctx.style_of(ctx.theme())).inner_margin(egui::Margin::same(18))
}

fn about(ctx: &egui::Context, state: &mut AppState) {
    let response = egui::Modal::new(egui::Id::new("about"))
        .frame(modal_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(380.0);
            ui.vertical_centered(|ui| {
                if let Some(logo) = state.logo(ctx) {
                    ui.add(egui::Image::new(&logo).fit_to_exact_size(egui::Vec2::splat(72.0)));
                }
                ui.add_space(6.0);
                ui.label(RichText::new("MusicScript").size(22.0).strong());
                ui.weak(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.add_space(8.0);
                ui.label("Python beat-making IDE, step sequencer, synthesizer and studio visualizer.");
                ui.weak("Rust interface (egui + wgpu) with a Python NumPy DSP worker.");
                ui.add_space(8.0);
                if let Some(info) = state.worker.info() {
                    ui.weak(format!(
                        "Python {} · MusicScript {} · Engine {}",
                        info.python, info.cadquery, info.ocp
                    ));
                }
                ui.hyperlink_to(
                    format!("{} Source code", icon::GITHUB_LOGO),
                    "https://github.com/igors93/CQ-editor",
                );
                ui.add_space(10.0);
                if ui.button("Close").clicked() {
                    ui.close();
                }
            });
        });
    if response.should_close() {
        state.dialogs.about = false;
    }
}

/// Returns the user's choice once made.
fn unsaved_changes(ctx: &egui::Context, name: &str) -> Option<UnsavedChoice> {
    let mut choice = None;
    let response = egui::Modal::new(egui::Id::new("unsaved_changes"))
        .frame(modal_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(360.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(icon::WARNING)
                        .size(28.0)
                        .color(ui.visuals().warn_fg_color),
                );
                ui.vertical(|ui| {
                    ui.label(RichText::new("Unsaved changes").strong().size(16.0));
                    ui.label(format!("Save the changes to {name} first?"));
                });
            });
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Cancel").clicked() {
                    choice = Some(UnsavedChoice::Cancel);
                }
                if ui.button("Don't save").clicked() {
                    choice = Some(UnsavedChoice::Discard);
                }
                if ui
                    .add(
                        egui::Button::new(RichText::new("Save").strong())
                            .fill(ui.visuals().selection.bg_fill),
                    )
                    .clicked()
                {
                    choice = Some(UnsavedChoice::Save);
                }
            });
        });
    if response.should_close() && choice.is_none() {
        choice = Some(UnsavedChoice::Cancel);
    }
    choice
}

/// Offers to read a file that is not UTF-8 as Windows-1252. Returns whether
/// to open it once the user decided.
fn legacy_encoding(ctx: &egui::Context, path: &Path) -> Option<bool> {
    let mut choice = None;
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let response = egui::Modal::new(egui::Id::new("legacy_encoding"))
        .frame(modal_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(440.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(icon::WARNING)
                        .size(28.0)
                        .color(ui.visuals().warn_fg_color),
                );
                ui.vertical(|ui| {
                    ui.label(RichText::new("Not a UTF-8 file").strong().size(16.0));
                    ui.label(format!("{name} is not valid UTF-8 text."))
                        .on_hover_text(path.display().to_string());
                    ui.label(
                        "It can be opened as Windows-1252 (Western European), the usual \
                         encoding of older Windows editors. Saving then converts it to \
                         UTF-8, the encoding Python expects.",
                    );
                    ui.weak("Opening does not change the file.");
                });
            });
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Cancel").clicked() {
                    choice = Some(false);
                }
                if ui
                    .add(
                        egui::Button::new(RichText::new("Open as Windows-1252").strong())
                            .fill(ui.visuals().selection.bg_fill),
                    )
                    .clicked()
                {
                    choice = Some(true);
                }
            });
        });
    if response.should_close() && choice.is_none() {
        choice = Some(false);
    }
    choice
}

/// Returns a resolution only after an explicit choice. Comparing keeps the
/// dialog open and Escape takes the lossless "keep local" path.
fn external_change(
    ctx: &egui::Context,
    name: &str,
    local_text: &str,
    change: &mut ExternalChange,
) -> Option<ExternalChangeChoice> {
    let mut choice = None;
    let response = egui::Modal::new(egui::Id::new("external_change"))
        .frame(modal_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(if change.show_comparison { 760.0 } else { 500.0 });
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(icon::WARNING)
                        .size(28.0)
                        .color(ui.visuals().warn_fg_color),
                );
                ui.vertical(|ui| {
                    ui.label(RichText::new("File changed on disk").strong().size(16.0));
                    ui.label(format!(
                        "{name} was changed by another program while you have unsaved edits."
                    ));
                    ui.weak("Your editor contents have not been replaced.");
                });
            });

            if change.show_comparison {
                ui.add_space(10.0);
                ui.columns(2, |columns| {
                    let show_version =
                        |column: &mut egui::Ui, title: &str, text: &str, id: &str| {
                            column.label(RichText::new(title).strong());
                            egui::Frame::group(column.style()).show(column, |ui| {
                                egui::ScrollArea::both().id_salt(id).max_height(300.0).show(
                                    ui,
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(RichText::new(text).monospace())
                                                .selectable(true),
                                        );
                                    },
                                );
                            });
                        };
                    show_version(&mut columns[0], "Your version", local_text, "local_change");
                    show_version(
                        &mut columns[1],
                        "Version on disk",
                        &change.disk.text,
                        "disk_change",
                    );
                });
            }

            ui.add_space(12.0);
            ui.weak(
                "Reload creates a recovery backup before replacing the editor contents. \
                 Saving a copy preserves your version separately, then reloads the disk version.",
            );
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new("Keep my version").strong())
                            .fill(ui.visuals().selection.bg_fill),
                    )
                    .clicked()
                {
                    choice = Some(ExternalChangeChoice::KeepLocal);
                }
                if ui.button("Reload from disk").clicked() {
                    choice = Some(ExternalChangeChoice::Reload);
                }
                if ui.button("Save a copy…").clicked() {
                    choice = Some(ExternalChangeChoice::SaveCopy);
                }
                let compare = if change.show_comparison {
                    "Hide comparison"
                } else {
                    "Compare changes"
                };
                if ui.button(compare).clicked() {
                    change.show_comparison = !change.show_comparison;
                }
            });
        });
    if response.should_close() && choice.is_none() {
        choice = Some(ExternalChangeChoice::KeepLocal);
    }
    choice
}

fn recovery_session(
    ctx: &egui::Context,
    snapshot: &RecoverySnapshot,
) -> Option<RecoverySessionChoice> {
    let mut choice = None;
    egui::Modal::new(egui::Id::new("recovery_session"))
        .frame(modal_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(560.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(icon::LIFEBUOY)
                        .size(28.0)
                        .color(ui.visuals().warn_fg_color),
                );
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("Unsaved work can be recovered")
                            .strong()
                            .size(16.0),
                    );
                    ui.label(format!(
                        "CQ-editor found an interrupted session for {}.",
                        snapshot.display_name()
                    ));
                    ui.weak(format!("Recovery checkpoint: {}", snapshot_time(snapshot)));
                });
            });
            ui.add_space(10.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                egui::ScrollArea::both()
                    .id_salt("recovery_preview")
                    .max_height(180.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(&snapshot.text).monospace())
                                .selectable(true),
                        );
                    });
            });
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new("Restore session").strong())
                            .fill(ui.visuals().selection.bg_fill),
                    )
                    .clicked()
                {
                    choice = Some(RecoverySessionChoice::Restore);
                }
                if ui.button("Discard recovery").clicked() {
                    choice = Some(RecoverySessionChoice::Discard);
                }
            });
        });
    choice
}

fn local_history(ctx: &egui::Context, history: &mut HistoryDialog) -> Option<HistoryChoice> {
    let mut choice = None;
    let response = egui::Modal::new(egui::Id::new("local_history"))
        .frame(modal_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(760.0);
            ui.label(RichText::new("Local history").strong().size(16.0));
            ui.weak("Recovery checkpoints are retained locally; the newest 20 versions are kept per document.");
            ui.add_space(10.0);
            ui.columns(2, |columns| {
                columns[0].set_width(220.0);
                egui::ScrollArea::vertical()
                    .id_salt("history_versions")
                    .max_height(360.0)
                    .show(&mut columns[0], |ui| {
                        for (index, snapshot) in history.versions.iter().enumerate() {
                            if ui
                                .selectable_label(
                                    history.selected == index,
                                    snapshot_time(snapshot),
                                )
                                .clicked()
                            {
                                history.selected = index;
                            }
                        }
                    });
                if let Some(snapshot) = history.versions.get(history.selected) {
                    columns[1].label(RichText::new(snapshot.display_name()).strong());
                    egui::Frame::group(columns[1].style()).show(&mut columns[1], |ui| {
                        egui::ScrollArea::both()
                            .id_salt("history_preview")
                            .max_height(330.0)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(RichText::new(&snapshot.text).monospace())
                                        .selectable(true),
                                );
                            });
                    });
                }
            });
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !history.versions.is_empty(),
                        egui::Button::new(RichText::new("Restore this version").strong())
                            .fill(ui.visuals().selection.bg_fill),
                    )
                    .clicked()
                    && let Some(snapshot) = history.versions.get(history.selected)
                {
                    choice = Some(HistoryChoice::Restore(snapshot.clone()));
                }
                if ui.button("Close").clicked() {
                    choice = Some(HistoryChoice::Close);
                }
            });
        });
    if response.should_close() && choice.is_none() {
        choice = Some(HistoryChoice::Close);
    }
    choice
}

fn snapshot_time(snapshot: &RecoverySnapshot) -> String {
    let time = std::time::UNIX_EPOCH
        .checked_add(std::time::Duration::from_millis(snapshot.timestamp_ms))
        .unwrap_or(std::time::UNIX_EPOCH);
    let local: chrono::DateTime<chrono::Local> = time.into();
    local.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Returns `true` once dismissed.
fn message_box(ctx: &egui::Context, message: &Message) -> bool {
    let mut dismissed = false;
    let response = egui::Modal::new(egui::Id::new("message_box"))
        .frame(modal_frame(ctx))
        .show(ctx, |ui| {
            ui.set_max_width(460.0);
            let (glyph, color) = match message.kind {
                MessageKind::Info => (icon::INFO, ui.visuals().hyperlink_color),
                MessageKind::Error => (icon::X_CIRCLE, ui.visuals().error_fg_color),
            };
            ui.horizontal(|ui| {
                ui.label(RichText::new(glyph).size(26.0).color(color));
                ui.label(RichText::new(&message.title).strong().size(16.0));
            });
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    ui.add(egui::Label::new(&message.text).selectable(true));
                });
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("OK").clicked() {
                    dismissed = true;
                }
            });
        });
    dismissed || response.should_close()
}

/// Why Python is not available, for [`python_problem`].
pub struct PythonProblem {
    pub problem: Option<Problem>,
    pub reason: String,
    /// The interpreter that failed.
    pub python: Option<PathBuf>,
}

impl PythonProblem {
    /// Title and advice.
    pub fn explanation(&self) -> (&'static str, &'static str) {
        match self.problem {
            Some(Problem::NoPython) => (
                "Python was not found",
                "CQ-editor runs your scripts with a Python that has CadQuery. Install one \
                 (see the installation guide), or choose an interpreter.",
            ),
            Some(Problem::NoCadQuery) => (
                "CadQuery is not available",
                "Install CadQuery in that environment (python -m pip install cadquery), \
                 or choose another interpreter.",
            ),
            Some(Problem::Exited) | None => (
                "Python stopped before it was ready",
                "The Log panel shows what it printed. Choose another interpreter, or retry.",
            ),
        }
    }
}

/// Most detected interpreters listed at once.
const MAX_LISTED_INTERPRETERS: usize = 8;

/// Floating hint shown while the worker could not start, with ways out:
/// interpreters found on this computer, picking one, the installation
/// guide, the preferences and a retry.
pub fn python_problem(
    ctx: &egui::Context,
    problem: &PythonProblem,
    candidates: &[PathBuf],
    actions: &mut Vec<Action>,
) {
    let (title, advice) = problem.explanation();
    let others: Vec<&PathBuf> = candidates
        .iter()
        .filter(|path| Some(*path) != problem.python.as_ref())
        .take(MAX_LISTED_INTERPRETERS)
        .collect();
    egui::Area::new(egui::Id::new("python_problem"))
        .anchor(Align2::CENTER_BOTTOM, [0.0, -40.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.set_max_width(560.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(icon::WARNING_OCTAGON)
                                .size(22.0)
                                .color(ui.visuals().error_fg_color),
                        );
                        ui.vertical(|ui| {
                            ui.label(RichText::new(title).strong());
                            ui.label(&problem.reason);
                            if let Some(python) = &problem.python {
                                ui.weak(format!("Interpreter: {}", python.display()));
                            }
                            ui.weak(advice);
                        });
                    });
                    if !others.is_empty() {
                        ui.add_space(4.0);
                        ui.label("Interpreters found on this computer:");
                        for path in others {
                            if ui
                                .link(format!("{}  {}", icon::FILE_PY, path.display()))
                                .on_hover_text("Restart Python with this interpreter")
                                .clicked()
                            {
                                actions.push(Action::UsePython(path.clone()));
                            }
                        }
                    }
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .button(format!("{} Choose interpreter…", icon::FOLDER_OPEN))
                            .clicked()
                            && let Some(path) = rfd::FileDialog::new()
                                .set_title("Python interpreter with CadQuery")
                                .pick_file()
                        {
                            actions.push(Action::UsePython(path));
                        }
                        if ui
                            .button(format!("{} Installation guide", icon::BOOK_OPEN_TEXT))
                            .clicked()
                        {
                            actions.push(Action::InstallationGuide);
                        }
                        if ui
                            .button(format!("{} Preferences…", icon::GEAR_SIX))
                            .clicked()
                        {
                            actions.push(Action::Preferences);
                        }
                        if ui
                            .button(format!("{} Retry", icon::ARROW_CLOCKWISE))
                            .clicked()
                        {
                            actions.push(Action::RestartPython);
                        }
                    });
                });
        });
}
