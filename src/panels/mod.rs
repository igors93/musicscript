//! The dockable panels and their default arrangement.

mod console;
mod inspector;
mod log;
mod measurements;
mod objects;
mod parameters;
mod traceback;
mod variables;

pub use console::{ConsoleState, EntryKind};
pub use inspector::InspectorState;
pub use log::LogView;
pub use measurements::{MeasurementState, SelectorState, SelectorUse};
pub use parameters::ParametersState;

use egui_dock::{DockState, NodeIndex, TabViewer};
use egui_phosphor::regular as icon;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tab {
    Editor,
    Viewer,
    Objects,
    Measurements,
    Parameters,
    Variables,
    Inspector,
    Console,
    Traceback,
    Log,
}

impl Tab {
    pub const ALL: [Tab; 10] = [
        Tab::Editor,
        Tab::Viewer,
        Tab::Objects,
        Tab::Measurements,
        Tab::Parameters,
        Tab::Variables,
        Tab::Inspector,
        Tab::Console,
        Tab::Traceback,
        Tab::Log,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Editor => "Beat Script",
            Tab::Viewer => "Studio View",
            Tab::Objects => "Tracks & Mixer",
            Tab::Measurements => "Audio Analyzer",
            Tab::Parameters => "Parameters",
            Tab::Variables => "Variables",
            Tab::Inspector => "Beat Inspector",
            Tab::Console => "Console",
            Tab::Traceback => "Traceback",
            Tab::Log => "Log",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Tab::Editor => icon::CODE,
            Tab::Viewer => icon::MUSIC_NOTES,
            Tab::Objects => icon::FADERS,
            Tab::Measurements => icon::WAVE_SINE,
            Tab::Parameters => icon::SLIDERS_HORIZONTAL,
            Tab::Variables => icon::BRACKETS_CURLY,
            Tab::Inspector => icon::STACK,
            Tab::Console => icon::TERMINAL_WINDOW,
            Tab::Traceback => icon::WARNING_CIRCLE,
            Tab::Log => icon::LIST_BULLETS,
        }
    }
}

/// Editor on the left, Studio view in the middle, Tracks & Mixer and Parameters on the
/// right and console, traceback and log at the bottom.
pub fn default_layout() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Viewer]);
    let tree = dock.main_surface_mut();
    // Fractions give the share of the left (or top) child of each split.
    let [top, bottom] = tree.split_below(NodeIndex::root(), 0.76, vec![Tab::Console]);
    tree.split_right(bottom, 0.45, vec![Tab::Traceback, Tab::Log]);
    let [viewer, _editor] = tree.split_left(top, 0.34, vec![Tab::Editor]);
    let [_viewer, objects] = tree.split_right(viewer, 0.72, vec![Tab::Objects]);
    tree.split_below(
        objects,
        0.55,
        vec![
            Tab::Parameters,
            Tab::Measurements,
            Tab::Inspector,
            Tab::Variables,
        ],
    );
    dock
}

/// Shows the panels; holds what they need from the application.
pub struct TabContext<'a> {
    pub state: &'a mut AppState,
    pub frame: &'a eframe::Frame,
}

impl TabViewer for TabContext<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        let mut title = format!("{}  {}", tab.icon(), tab.title());
        match tab {
            Tab::Editor => {
                title = format!("{}  {}", tab.icon(), self.state.document.display_name());
                if self.state.document.is_modified() {
                    title.push_str(" •");
                }
            }
            Tab::Traceback if self.state.traceback.is_some() => title.push_str(" •"),
            _ => {}
        }
        title.into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        constrain_to_tab(ui);
        match tab {
            Tab::Editor => self.state.editor_ui(ui),
            Tab::Viewer => self.state.viewer_ui(ui, self.frame),
            Tab::Objects => objects::ui(ui, self.state),
            Tab::Measurements => measurements::ui(ui, self.state),
            Tab::Parameters => parameters::ui(ui, self.state),
            Tab::Variables => variables::ui(ui, self.state),
            Tab::Inspector => inspector::ui(ui, self.state),
            Tab::Console => console::ui(ui, self.state),
            Tab::Traceback => traceback::ui(ui, self.state),
            Tab::Log => log::ui(ui, self.state),
        }
    }

    fn is_closeable(&self, tab: &Tab) -> bool {
        *tab != Tab::Viewer
    }

    fn scroll_bars(&self, _tab: &Tab) -> [bool; 2] {
        // Every panel scrolls its own content.
        [false, false]
    }
}

/// Keep every panel's paint inside the body assigned by `egui_dock`.
///
/// The dock creates tab bodies as fresh [`egui::Ui`]s. Their maximum rectangle
/// is the tab body, but their inherited clip rectangle can be as large as the
/// whole dock area. Most containers narrow it again; widgets that do not can
/// otherwise paint text over a split separator or a neighbouring tab bar.
fn constrain_to_tab(ui: &mut egui::Ui) {
    ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
}

/// Centered, dimmed text for empty panels.
///
/// Laid out with centered rows rather than a justified layout, which
/// stretched the spaces of wrapped lines unevenly.
pub(crate) fn empty_state(ui: &mut egui::Ui, icon: &str, text: &str) {
    let rect = ui.available_rect_before_wrap();
    ui.allocate_rect(rect, egui::Sense::hover());
    let color = ui.visuals().weak_text_color();
    let job = empty_state_job(ui.style(), &format!("{icon}  {text}"), color, rect.width());
    let galley = ui.painter().layout_job(job);
    // Centered rows are laid out around x = 0.
    let top = (rect.center().y - galley.size().y / 2.0).max(rect.top());
    ui.painter()
        .galley(egui::pos2(rect.center().x, top), galley, color);
}

fn empty_state_job(
    style: &egui::Style,
    text: &str,
    color: egui::Color32,
    width: f32,
) -> egui::text::LayoutJob {
    let font = egui::TextStyle::Body.resolve(style);
    let mut job =
        egui::text::LayoutJob::simple(text.to_owned(), font, color, (width - 24.0).max(40.0));
    job.halign = egui::Align::Center;
    job
}

#[cfg(test)]
#[path = "../../tests/rust/panels/test_empty_state.rs"]
mod tests;
