//! The toolbar below the menu, with the same groups as the original:
//! file, objects, run/debug, inspection and view commands.

use egui::{Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Vec2};
use egui_phosphor::regular as icon;
use glam::Vec3;

use crate::app::AppState;
use crate::commands::{self, Action};
use crate::viewer::camera::StandardView;
use crate::viewer::{DisplayMode, SelectionMode};
use crate::worker::protocol::DebugAction;

const BUTTON_SIZE: Vec2 = Vec2::new(28.0, 26.0);

pub fn ui(ui: &mut egui::Ui, state: &mut AppState) {
    let mut actions = Vec::new();
    let paused = state.is_paused();

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let actions = &mut actions;
        tool(
            ui,
            actions,
            icon::FILE_PLUS,
            "New",
            Action::NewFile,
            true,
            false,
        );
        tool(
            ui,
            actions,
            icon::FOLDER_OPEN,
            "Open",
            Action::OpenFile,
            true,
            false,
        );
        tool(
            ui,
            actions,
            icon::FLOPPY_DISK,
            "Save",
            Action::Save,
            true,
            false,
        );
        tool(
            ui,
            actions,
            icon::FLOPPY_DISK_BACK,
            "Save as",
            Action::SaveAs,
            true,
            false,
        );
        tool(
            ui,
            actions,
            icon::ARROWS_CLOCKWISE,
            "Automatic reload and preview",
            Action::ToggleAutoreload,
            true,
            state.settings.editor.autoreload,
        );
        separator(ui);

        let has_objects = !state.scene.is_empty();
        tool(
            ui,
            actions,
            icon::TRASH,
            "Clear all tracks",
            Action::ClearObjects,
            has_objects,
            false,
        );
        tool(
            ui,
            actions,
            icon::X_SQUARE,
            "Remove selected track",
            Action::RemoveSelected,
            !state.scene.selection.is_empty(),
            false,
        );
        separator(ui);

        tool(
            ui,
            actions,
            icon::PLAY,
            "Synthesize & Play Beat",
            Action::Render,
            true,
            false,
        );
        tool(
            ui,
            actions,
            icon::TIMER,
            "Profile: synthesize and show each line's time in the gutter",
            Action::Profile,
            true,
            !state.editor.line_profile.is_empty(),
        );
        tool(
            ui,
            actions,
            icon::BUG,
            "Debug",
            Action::ToggleDebug,
            true,
            state.is_debugging(),
        );
        tool(
            ui,
            actions,
            icon::ARROW_BEND_DOWN_RIGHT,
            "Step",
            Action::Debug(DebugAction::Step),
            paused,
            false,
        );
        tool(
            ui,
            actions,
            icon::ARROW_ELBOW_DOWN_RIGHT,
            "Step in",
            Action::Debug(DebugAction::StepIn),
            paused,
            false,
        );
        tool(
            ui,
            actions,
            icon::ARROW_ELBOW_UP_LEFT,
            "Step out",
            Action::Debug(DebugAction::StepOut),
            paused,
            false,
        );
        tool(
            ui,
            actions,
            icon::FAST_FORWARD,
            "Continue",
            Action::Debug(DebugAction::Continue),
            paused,
            false,
        );
        tool(
            ui,
            actions,
            icon::STOP,
            "Stop",
            Action::Stop,
            state.can_stop(),
            false,
        );
        separator(ui);

        tool(
            ui,
            actions,
            icon::STACK,
            "Inspect Beat Track Signal Chain",
            Action::ToggleInspect,
            true,
            state.inspector.show_in_view,
        );
        separator(ui);

        tool(
            ui,
            actions,
            icon::CORNERS_OUT,
            "Fit",
            Action::Fit,
            true,
            false,
        );
        for view in StandardView::ALL {
            let action = Action::View(view);
            let tooltip = tooltip_text(ui, view.label(), &action);
            if view_button(ui, view).on_hover_text(tooltip).clicked() {
                actions.push(action);
            }
        }
        let views = icon_button(ui, icon::BOOKMARKS_SIMPLE, true, false)
            .on_hover_text("Named views: save the camera and come back to it");
        egui::Popup::menu(&views)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| super::named_views::manager(ui, state, actions));
        separator(ui);

        for mode in DisplayMode::ALL {
            let (glyph, tooltip) = match mode {
                DisplayMode::Wireframe => (icon::CUBE_TRANSPARENT, "Wireframe"),
                DisplayMode::Shaded => (icon::CUBE, "Shaded"),
                DisplayMode::XRay => (icon::INTERSECT_SQUARE, "X-ray: see-through faces"),
                DisplayMode::HiddenLine => (
                    icon::RECTANGLE_DASHED,
                    "Hidden line: visible edges, hidden ones dashed",
                ),
            };
            tool(
                ui,
                actions,
                glyph,
                tooltip,
                Action::Display(mode),
                true,
                state.viewer.display_mode == mode,
            );
        }
        tool(
            ui,
            actions,
            icon::SCISSORS,
            "Section view: cut the model with a plane",
            Action::ToggleSection,
            true,
            state.viewer.section.enabled,
        );
        tool(
            ui,
            actions,
            icon::ARROWS_OUT,
            "Exploded view: move the parts apart",
            Action::ToggleExplode,
            true,
            state.viewer.explode_controls,
        );
        tool(
            ui,
            actions,
            icon::CAMERA,
            "Screenshot",
            Action::Screenshot,
            true,
            false,
        );
        separator(ui);

        ui.label("Select:");
        egui::ComboBox::from_id_salt("topology_selection_mode")
            .width(72.0)
            .selected_text(state.viewer.selection_mode.label())
            .show_ui(ui, |ui| {
                for mode in SelectionMode::ALL {
                    if ui
                        .selectable_label(state.viewer.selection_mode == mode, mode.label())
                        .on_hover_text(match mode {
                            SelectionMode::Auto => "Nearest vertex, edge or face",
                            SelectionMode::Object => "Whole displayed object",
                            SelectionMode::Face => "Faces only",
                            SelectionMode::Edge => "Edges only",
                            SelectionMode::Vertex => "Vertices only",
                        })
                        .clicked()
                    {
                        actions.push(Action::SelectionMode(mode));
                    }
                }
            });
    });

    state.queue(actions);
}

fn tool(
    ui: &mut egui::Ui,
    actions: &mut Vec<Action>,
    glyph: &str,
    tooltip: &str,
    action: Action,
    enabled: bool,
    selected: bool,
) {
    if icon_button(ui, glyph, enabled, selected)
        .on_hover_text(tooltip_text(ui, tooltip, &action))
        .clicked()
    {
        actions.push(action);
    }
}

fn tooltip_text(ui: &egui::Ui, label: &str, action: &Action) -> String {
    match commands::shortcut(action) {
        Some(shortcut) => format!("{label} ({})", ui.ctx().format_shortcut(&shortcut)),
        None => label.to_owned(),
    }
}

fn separator(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);
}

fn button_frame(ui: &mut egui::Ui, enabled: bool, selected: bool) -> (Rect, Response) {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(BUTTON_SIZE, sense);
    let visuals = ui.visuals();
    let fill = if selected {
        visuals.selection.bg_fill
    } else if enabled && response.is_pointer_button_down_on() {
        visuals.widgets.active.weak_bg_fill
    } else if enabled && response.hovered() {
        visuals.widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 5.0, fill);
    (rect, response)
}

fn icon_button(ui: &mut egui::Ui, glyph: &str, enabled: bool, selected: bool) -> Response {
    let (rect, response) = button_frame(ui, enabled, selected);
    let color = if enabled {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color().gamma_multiply(0.6)
    };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(17.0),
        color,
    );
    response
}

/// A small isometric cube with the face of `view` highlighted; hidden faces
/// (back, left, bottom) are drawn through the cube with dashed edges.
fn view_button(ui: &mut egui::Ui, view: StandardView) -> Response {
    let (rect, response) = button_frame(ui, true, false);
    let text = ui.visuals().text_color();
    let accent = ui.visuals().hyperlink_color;
    paint_view_icon(ui.painter(), rect.shrink(5.0), view, text, accent);
    response
}

fn paint_view_icon(
    painter: &egui::Painter,
    rect: Rect,
    view: StandardView,
    line: Color32,
    accent: Color32,
) {
    let rotation = StandardView::Iso.rotation().inverse();
    let scale = rect.height() / 3.2;
    let project = |p: Vec3| {
        let v = rotation * (p * 2.0 - Vec3::ONE);
        Pos2::new(rect.center().x + v.x * scale, rect.center().y - v.y * scale)
    };
    // Faces as (outward normal, corners).
    let faces: [(Vec3, [Vec3; 4]); 6] = [
        (
            Vec3::Z,
            [
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::new(1.0, 1.0, 1.0),
                Vec3::new(0.0, 1.0, 1.0),
            ],
        ),
        (
            Vec3::NEG_Z,
            [
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
            ],
        ),
        (
            Vec3::NEG_Y,
            [
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::new(0.0, 0.0, 1.0),
            ],
        ),
        (
            Vec3::Y,
            [
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 1.0, 1.0),
                Vec3::new(1.0, 1.0, 1.0),
                Vec3::new(1.0, 1.0, 0.0),
            ],
        ),
        (
            Vec3::X,
            [
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(1.0, 1.0, 1.0),
                Vec3::new(1.0, 0.0, 1.0),
            ],
        ),
        (
            Vec3::NEG_X,
            [
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(0.0, 1.0, 1.0),
                Vec3::new(0.0, 1.0, 0.0),
            ],
        ),
    ];
    let eye = StandardView::Iso.orientation().0;
    let highlighted = match view {
        StandardView::Iso => None,
        other => Some(other.orientation().0),
    };

    // Hidden geometry first, so visible faces cover it.
    for (normal, corners) in &faces {
        let visible = normal.dot(eye) > 0.0;
        let points: Vec<Pos2> = corners.iter().map(|c| project(*c)).collect();
        let is_target = highlighted.is_some_and(|h| h == *normal);
        if !visible {
            if is_target {
                painter.add(Shape::convex_polygon(
                    points.clone(),
                    accent.gamma_multiply(0.55),
                    Stroke::NONE,
                ));
            }
            for i in 0..4 {
                let dashes = Shape::dashed_line(
                    &[points[i], points[(i + 1) % 4]],
                    Stroke::new(0.8, line.gamma_multiply(0.45)),
                    1.5,
                    1.5,
                );
                painter.extend(dashes);
            }
        }
    }
    for (normal, corners) in &faces {
        if normal.dot(eye) <= 0.0 {
            continue;
        }
        let mut points: Vec<Pos2> = corners.iter().map(|c| project(*c)).collect();
        let area: f32 = points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .map(|(a, b)| a.x * b.y - b.x * a.y)
            .sum();
        if area < 0.0 {
            points.reverse();
        }
        let fill = if highlighted.is_some_and(|h| h == *normal) || view == StandardView::Iso {
            accent.gamma_multiply(if view == StandardView::Iso { 0.45 } else { 0.9 })
        } else {
            Color32::TRANSPARENT
        };
        painter.add(Shape::convex_polygon(points, fill, Stroke::new(1.0, line)));
    }
}
