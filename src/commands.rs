//! Everything the user can ask for, from menus, the toolbar, panels and
//! keyboard shortcuts. Widgets only queue actions; the `app` module
//! executes them after the frame's UI has been built.

use std::path::PathBuf;

use egui::{Key, KeyboardShortcut, Modifiers};

use crate::panels::Tab;
use crate::scene::{NodeKey, TopologySelection};
use crate::viewer::camera::StandardView;
use crate::viewer::{DisplayMode, SelectionMode};
use crate::worker::protocol::{DebugAction, ExportFormat};

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    // File
    NewFile,
    OpenFile,
    OpenPath(PathBuf),
    OpenExample(usize),
    Save,
    SaveAs,
    LocalHistory,
    ToggleAutoreload,
    Quit,
    // Edit
    Undo,
    Redo,
    ToggleComment,
    Autocomplete,
    Find,
    FindNext,
    FindPrevious,
    Replace,
    /// Opens the *Go to line* bar.
    GoToLinePrompt,
    /// The CadQuery selector of the picked entities, to the clipboard.
    CopySelector,
    /// The CadQuery selector of the picked entities, at the editor's cursor.
    InsertSelector,
    /// Python repeating the current measurement, to the clipboard.
    CopyMeasurementCode,
    Preferences,
    // Run
    Render,
    /// Renders with the time of each line measured, shown in the gutter.
    Profile,
    /// Removes the profile's times from the gutter.
    ClearProfile,
    /// Starts a debugging session, or ends the running one.
    ToggleDebug,
    Debug(DebugAction),
    /// Aborts whatever the worker is doing (by restarting it).
    Stop,
    /// Stops meshing the running render's objects; the model shown stays.
    CancelMeshing,
    ClearConsole,
    ClearLog,
    RestartPython,
    /// Restarts Python with this interpreter and keeps it in the
    /// preferences.
    UsePython(PathBuf),
    /// Opens the dialog creating a Python environment with CadQuery.
    CreateEnvironment,
    /// Opens the dialog installing packages into the Python in use.
    InstallPackages,
    // View
    Fit,
    FitSelection,
    View(StandardView),
    Display(DisplayMode),
    SelectionMode(SelectionMode),
    ToggleProjection,
    /// Turns the section plane on or off.
    ToggleSection,
    ToggleGroundGrid,
    ToggleScaleBar,
    /// Spreads the parts of the model apart and shows the controls of the
    /// exploded view, or assembles the parts again.
    ToggleExplode,
    /// How far the exploded view spreads the parts (`Scene::set_explode`).
    SetExplode(f32),
    /// Shows only the selected objects, or ends the isolation.
    ToggleIsolate,
    /// Ends the isolation and makes every object visible.
    ShowAll,
    /// Saves the camera as a named view of the document: under this name
    /// (replacing a view of the same name), or the next "View N".
    SaveView(Option<String>),
    /// Moves the camera to the named view at this index.
    GoToView(usize),
    /// Stores the current camera in the named view at this index.
    UpdateView(usize),
    DeleteView(usize),
    TogglePanel(Tab),
    ResetLayout,
    // Tools
    Export(ExportFormat),
    ToggleInspect,
    /// Opens the screenshot and animation options.
    Screenshot,
    /// Asks where to save a screenshot with the chosen options.
    SaveImage,
    /// Asks where to save a turntable animation and starts it.
    ExportTurntable,
    CancelTurntable,
    Upload,
    // Help
    Documentation,
    InstallationGuide,
    CadQueryDocumentation,
    About,
    CheckUpdates,
    /// Opens the page of the newer CQ-editor release found.
    DownloadUpdate,
    // From panels and the view
    GoToLine(usize),
    Select {
        key: Option<NodeKey>,
        topology: Option<TopologySelection>,
        extend: bool,
    },
    SetVisible {
        key: NodeKey,
        visible: bool,
    },
    /// The script line that created an object, or one step of its
    /// construction stack as (frame, item) in the CQ Inspector.
    GoToSource {
        key: NodeKey,
        step: Option<(usize, Option<usize>)>,
    },
    /// The script line that created the current object.
    GoToSelectionSource,
    /// A new value for the parameter `name` assigned on `line` (1-based).
    SetParameter {
        name: String,
        line: usize,
        value: f64,
    },
    RemoveSelected,
    ClearObjects,
    InspectEntry {
        frame: usize,
        item: Option<usize>,
    },
    ConsoleSubmit(String),
    /// Tab in the console input.
    ConsoleComplete,
    /// Fresh console namespace.
    ResetConsole,
    StartUpload {
        url: String,
        file_type: String,
    },
}

const fn key(key: Key) -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::NONE, key)
}

const fn ctrl(key: Key) -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, key)
}

const fn shift(key: Key) -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::SHIFT, key)
}

const fn alt(key: Key) -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::ALT, key)
}

const fn ctrl_shift(key: Key) -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), key)
}

/// The shortcut shown next to an action in menus and tooltips.
pub fn shortcut(action: &Action) -> Option<KeyboardShortcut> {
    shortcuts()
        .into_iter()
        .chain(editor_shortcuts())
        .find(|(a, _)| a == action)
        .map(|(_, shortcut)| shortcut)
}

/// Shortcuts the code editor handles itself, only while it has the
/// keyboard focus: the console and the search fields keep their own undo.
/// Ctrl+Shift+Z also redoes.
pub fn editor_shortcuts() -> Vec<(Action, KeyboardShortcut)> {
    vec![(Action::Undo, ctrl(Key::Z)), (Action::Redo, ctrl(Key::Y))]
}

/// All global shortcuts, most specific first: egui ignores extra Shift/Alt
/// when matching, so Shift+F5 must be checked before F5.
pub fn shortcuts() -> Vec<(Action, KeyboardShortcut)> {
    let mut all = vec![
        (Action::NewFile, ctrl(Key::N)),
        (Action::OpenFile, ctrl(Key::O)),
        (Action::SaveAs, ctrl_shift(Key::S)),
        (Action::Save, ctrl(Key::S)),
        (Action::Quit, ctrl(Key::Q)),
        (Action::ToggleComment, ctrl(Key::Slash)),
        (Action::Autocomplete, ctrl(Key::Space)),
        (Action::Autocomplete, alt(Key::Slash)),
        (Action::Find, ctrl(Key::F)),
        (Action::Replace, ctrl(Key::H)),
        (Action::GoToLinePrompt, ctrl(Key::G)),
        (Action::InsertSelector, ctrl_shift(Key::I)),
        (Action::GoToSelectionSource, ctrl_shift(Key::G)),
        (Action::FindPrevious, alt(Key::F3)),
        (Action::FindNext, key(Key::F3)),
        (Action::Preferences, ctrl(Key::Comma)),
        (Action::Stop, ctrl_shift(Key::F5)),
        (Action::ToggleDebug, ctrl(Key::F5)),
        (Action::Render, key(Key::F5)),
        (Action::Profile, alt(Key::F5)),
        (Action::Debug(DebugAction::Step), ctrl(Key::F10)),
        (Action::Debug(DebugAction::StepOut), ctrl_shift(Key::F11)),
        (Action::Debug(DebugAction::StepIn), ctrl(Key::F11)),
        (Action::Debug(DebugAction::Continue), ctrl(Key::F12)),
        (Action::Fit, shift(Key::F1)),
        (Action::View(StandardView::Iso), shift(Key::F2)),
        (Action::View(StandardView::Top), shift(Key::F3)),
        (Action::View(StandardView::Bottom), shift(Key::F4)),
        (Action::View(StandardView::Front), shift(Key::F5)),
        (Action::View(StandardView::Back), shift(Key::F6)),
        (Action::View(StandardView::Left), shift(Key::F7)),
        (Action::View(StandardView::Right), shift(Key::F8)),
        (Action::Display(DisplayMode::Wireframe), shift(Key::F9)),
        (Action::Display(DisplayMode::Shaded), shift(Key::F10)),
        (Action::Display(DisplayMode::XRay), shift(Key::F11)),
        (Action::Display(DisplayMode::HiddenLine), shift(Key::F12)),
    ];
    let specificity = |s: &KeyboardShortcut| {
        let m = s.modifiers;
        u8::from(m.command || m.ctrl) + u8::from(m.shift) + u8::from(m.alt)
    };
    // Stable, so equally specific shortcuts keep their listed order.
    all.sort_by_key(|(_, s)| std::cmp::Reverse(specificity(s)));
    all
}

/// Queues the actions whose shortcuts were pressed this frame.
pub fn collect_shortcuts(ctx: &egui::Context, actions: &mut Vec<Action>) {
    ctx.input_mut(|input| {
        for (action, shortcut) in shortcuts() {
            if input.consume_shortcut(&shortcut) {
                actions.push(action);
            }
        }
    });
}

#[cfg(test)]
#[path = "../tests/rust/app/test_commands.rs"]
mod tests;
