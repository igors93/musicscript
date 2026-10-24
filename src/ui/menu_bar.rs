//! The main menu, organized like the original editor's.

use egui_phosphor::regular as icon;

use crate::app::AppState;
use crate::commands::{self, Action};
use crate::panels::Tab;
use crate::viewer::DisplayMode;
use crate::viewer::camera::StandardView;
use crate::worker::protocol::{DebugAction, ExportFormat};

pub fn ui(ui: &mut egui::Ui, state: &mut AppState, open_panels: &[Tab]) {
    let mut actions = Vec::new();
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| file_menu(ui, state, &mut actions));
        ui.menu_button("Edit", |ui| edit_menu(ui, state, &mut actions));
        ui.menu_button("Tools", |ui| tools_menu(ui, state, &mut actions));
        ui.menu_button("Run", |ui| run_menu(ui, state, &mut actions));
        ui.menu_button("View", |ui| view_menu(ui, state, open_panels, &mut actions));
        ui.menu_button("Help", |ui| help_menu(ui, state, &mut actions));
    });
    state.queue(actions);
}

/// A menu entry showing its keyboard shortcut.
fn item(ui: &mut egui::Ui, actions: &mut Vec<Action>, icon: &str, label: &str, action: Action) {
    item_enabled(ui, actions, icon, label, action, true);
}

fn item_enabled(
    ui: &mut egui::Ui,
    actions: &mut Vec<Action>,
    icon: &str,
    label: &str,
    action: Action,
    enabled: bool,
) {
    let mut button = egui::Button::new(format!("{icon}  {label}"));
    if let Some(shortcut) = commands::shortcut(&action) {
        button = button.shortcut_text(ui.ctx().format_shortcut(&shortcut));
    }
    if ui.add_enabled(enabled, button).clicked() {
        actions.push(action);
        ui.close();
    }
}

fn check_item(
    ui: &mut egui::Ui,
    actions: &mut Vec<Action>,
    checked: bool,
    label: &str,
    action: Action,
) {
    let mut button = egui::Button::new(format!(
        "{}  {label}",
        if checked {
            icon::CHECK_SQUARE
        } else {
            icon::SQUARE
        }
    ));
    if let Some(shortcut) = commands::shortcut(&action) {
        button = button.shortcut_text(ui.ctx().format_shortcut(&shortcut));
    }
    if ui.add(button).clicked() {
        actions.push(action);
        ui.close();
    }
}

fn file_menu(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<Action>) {
    item(ui, actions, icon::FILE_PLUS, "New", Action::NewFile);
    item(ui, actions, icon::FOLDER_OPEN, "Open…", Action::OpenFile);
    ui.menu_button(
        format!("{}  Open Recent", icon::CLOCK_COUNTER_CLOCKWISE),
        |ui| {
            if state.settings.recent_files.is_empty() {
                ui.weak("No recent files");
            }
            for path in &state.settings.recent_files {
                let name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                if ui
                    .button(name)
                    .on_hover_text(path.display().to_string())
                    .clicked()
                {
                    actions.push(Action::OpenPath(path.clone()));
                    ui.close();
                }
            }
        },
    );
    item(ui, actions, icon::FLOPPY_DISK, "Save", Action::Save);
    item(
        ui,
        actions,
        icon::FLOPPY_DISK_BACK,
        "Save As…",
        Action::SaveAs,
    );
    item(
        ui,
        actions,
        icon::CLOCK_COUNTER_CLOCKWISE,
        "Local History…",
        Action::LocalHistory,
    );
    ui.separator();
    check_item(
        ui,
        actions,
        state.settings.editor.autoreload,
        "Automatic Reload and Preview",
        Action::ToggleAutoreload,
    );
    ui.separator();
    ui.menu_button(format!("{}  Examples", icon::BOOK_OPEN), |ui| {
        for (index, example) in state.examples().iter().enumerate() {
            if ui.button(&example.title).clicked() {
                actions.push(Action::OpenExample(index));
                ui.close();
            }
        }
    });
    ui.separator();
    item(ui, actions, icon::SIGN_OUT, "Quit", Action::Quit);
}

fn edit_menu(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<Action>) {
    item_enabled(
        ui,
        actions,
        icon::ARROW_U_UP_LEFT,
        "Undo",
        Action::Undo,
        state.editor.can_undo(),
    );
    item_enabled(
        ui,
        actions,
        icon::ARROW_U_UP_RIGHT,
        "Redo",
        Action::Redo,
        state.editor.can_redo(),
    );
    ui.separator();
    item(
        ui,
        actions,
        icon::HASH,
        "Toggle Comment",
        Action::ToggleComment,
    );
    item(
        ui,
        actions,
        icon::LIGHTNING,
        "Auto-Complete",
        Action::Autocomplete,
    );
    item(ui, actions, icon::MAGNIFYING_GLASS, "Search", Action::Find);
    item(ui, actions, icon::SWAP, "Replace", Action::Replace);
    item(
        ui,
        actions,
        icon::LIST_NUMBERS,
        "Go to Line…",
        Action::GoToLinePrompt,
    );
    ui.separator();
    let picked = !state.scene.topology_selections.is_empty();
    item_enabled(
        ui,
        actions,
        icon::COPY,
        "Copy Selector",
        Action::CopySelector,
        picked,
    );
    item_enabled(
        ui,
        actions,
        icon::CURSOR_TEXT,
        "Insert Selector",
        Action::InsertSelector,
        picked,
    );
    item_enabled(
        ui,
        actions,
        icon::ARROW_SQUARE_OUT,
        "Go to Source",
        Action::GoToSelectionSource,
        state
            .scene
            .current()
            .is_some_and(|node| node.line.is_some()),
    );
    ui.separator();
    item(
        ui,
        actions,
        icon::GEAR_SIX,
        "Preferences…",
        Action::Preferences,
    );
}

fn tools_menu(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<Action>) {
    let has_objects = !state.scene.is_empty();
    for format in ExportFormat::AUDIO {
        item_enabled(
            ui,
            actions,
            icon::MUSIC_NOTES,
            &format!("Export as {}…", format.label()),
            Action::Export(format),
            has_objects,
        );
    }
    ui.separator();
    for format in ExportFormat::ALL {
        item_enabled(
            ui,
            actions,
            icon::EXPORT,
            &format!("Export 3D Roll as {}…", format.label()),
            Action::Export(format),
            has_objects,
        );
    }
    ui.separator();
    check_item(
        ui,
        actions,
        state.inspector.show_in_view,
        "Inspect Beat Signal Chain",
        Action::ToggleInspect,
    );
    item(ui, actions, icon::CAMERA, "Screenshot…", Action::Screenshot);
    item(
        ui,
        actions,
        icon::UPLOAD_SIMPLE,
        "Upload Project…",
        Action::Upload,
    );
}

fn run_menu(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<Action>) {
    let paused = state.is_paused();
    let debugging = state.is_debugging();
    item(ui, actions, icon::PLAY, "Synthesize & Play", Action::Render);
    item(ui, actions, icon::TIMER, "Profile", Action::Profile);
    item_enabled(
        ui,
        actions,
        icon::ERASER,
        "Clear Profile",
        Action::ClearProfile,
        !state.editor.line_profile.is_empty(),
    );
    item(
        ui,
        actions,
        icon::BUG,
        if debugging { "Stop Debugging" } else { "Debug" },
        Action::ToggleDebug,
    );
    item_enabled(
        ui,
        actions,
        icon::ARROW_BEND_DOWN_RIGHT,
        "Step",
        Action::Debug(DebugAction::Step),
        paused,
    );
    item_enabled(
        ui,
        actions,
        icon::ARROW_ELBOW_DOWN_RIGHT,
        "Step In",
        Action::Debug(DebugAction::StepIn),
        paused,
    );
    item_enabled(
        ui,
        actions,
        icon::ARROW_ELBOW_UP_LEFT,
        "Step Out",
        Action::Debug(DebugAction::StepOut),
        paused,
    );
    item_enabled(
        ui,
        actions,
        icon::FAST_FORWARD,
        "Continue",
        Action::Debug(DebugAction::Continue),
        paused,
    );
    item_enabled(
        ui,
        actions,
        icon::STOP,
        "Stop",
        Action::Stop,
        state.can_stop(),
    );
    ui.separator();
    item(
        ui,
        actions,
        icon::BROOM,
        "Clear Console",
        Action::ClearConsole,
    );
    item(ui, actions, icon::ERASER, "Clear Log", Action::ClearLog);
    ui.separator();
    item(
        ui,
        actions,
        icon::ARROW_CLOCKWISE,
        "Restart Python",
        Action::RestartPython,
    );
}

fn view_menu(ui: &mut egui::Ui, state: &AppState, open_panels: &[Tab], actions: &mut Vec<Action>) {
    item(ui, actions, icon::CORNERS_OUT, "Fit", Action::Fit);
    item(
        ui,
        actions,
        icon::MAGNIFYING_GLASS_PLUS,
        "Zoom to Selection",
        Action::FitSelection,
    );
    for view in StandardView::ALL {
        item(ui, actions, icon::CUBE, view.label(), Action::View(view));
    }
    ui.menu_button(format!("{}  Named Views", icon::BOOKMARKS_SIMPLE), |ui| {
        super::named_views::submenu(ui, state, actions);
    });
    ui.separator();
    for mode in DisplayMode::ALL {
        check_item(
            ui,
            actions,
            state.viewer.display_mode == mode,
            mode.label(),
            Action::Display(mode),
        );
    }
    check_item(
        ui,
        actions,
        state.settings.viewer.projection == crate::settings::Projection::Perspective,
        "Perspective Projection",
        Action::ToggleProjection,
    );
    check_item(
        ui,
        actions,
        state.viewer.section.enabled,
        "Section View",
        Action::ToggleSection,
    );
    check_item(
        ui,
        actions,
        state.viewer.explode_controls,
        "Exploded View",
        Action::ToggleExplode,
    );
    check_item(
        ui,
        actions,
        state.settings.viewer.ground_grid,
        "Ground Grid",
        Action::ToggleGroundGrid,
    );
    check_item(
        ui,
        actions,
        state.settings.viewer.scale_bar,
        "Scale Bar",
        Action::ToggleScaleBar,
    );
    ui.separator();
    let isolating = state.scene.is_isolating();
    item_enabled(
        ui,
        actions,
        icon::SELECTION_FOREGROUND,
        if isolating {
            "End Isolation"
        } else {
            "Isolate Selection"
        },
        Action::ToggleIsolate,
        isolating || !state.scene.selection.is_empty(),
    );
    item(ui, actions, icon::EYE, "Show All", Action::ShowAll);
    ui.separator();
    for tab in Tab::ALL.into_iter().filter(|t| *t != Tab::Viewer) {
        check_item(
            ui,
            actions,
            open_panels.contains(&tab),
            tab.title(),
            Action::TogglePanel(tab),
        );
    }
    ui.separator();
    item(
        ui,
        actions,
        icon::LAYOUT,
        "Reset Layout",
        Action::ResetLayout,
    );
}

fn help_menu(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<Action>) {
    item(
        ui,
        actions,
        icon::LIFEBUOY,
        "Documentation",
        Action::Documentation,
    );
    item(
        ui,
        actions,
        icon::BOOK_OPEN_TEXT,
        "Installation Guide",
        Action::InstallationGuide,
    );
    item(
        ui,
        actions,
        icon::BOOKS,
        "CadQuery Documentation",
        Action::CadQueryDocumentation,
    );
    ui.separator();
    if let Some(release) = crate::updates::available(&state.settings.general) {
        item(
            ui,
            actions,
            icon::DOWNLOAD_SIMPLE,
            &format!("Download CQ-editor {}…", release.tag),
            Action::DownloadUpdate,
        );
    }
    item(
        ui,
        actions,
        icon::ARROW_CIRCLE_UP,
        "Check for Updates",
        Action::CheckUpdates,
    );
    item(ui, actions, icon::INFO, "About", Action::About);
}
