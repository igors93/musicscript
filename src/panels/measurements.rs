use egui_phosphor::regular as icon;

use crate::app::AppState;
use crate::commands::Action;
use crate::scene::TopologySelection;
use crate::worker::protocol::{
    BoundingBox, EntityMeasurement, Measurement, ObjectMeasurement, PairMeasurement, RequestId,
    SelectorCandidate, SelectorReply, TopologyKind,
};

#[derive(Default)]
pub struct MeasurementState {
    pub result: Option<Measurement>,
    pub loading: bool,
    pub error: Option<String>,
    pub request_id: Option<RequestId>,
}

impl MeasurementState {
    pub fn clear(&mut self) {
        self.result = None;
        self.loading = false;
        self.error = None;
        self.request_id = None;
    }
}

/// What to do with a selector once the worker found it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectorUse {
    Copy,
    Insert,
}

/// The CadQuery selector of the current topology selection.
#[derive(Default)]
pub struct SelectorState {
    pub reply: Option<SelectorReply>,
    /// The candidate in use, an index into `reply.candidates`.
    pub chosen: usize,
    pub loading: bool,
    pub error: Option<String>,
    pub request_id: Option<RequestId>,
    /// Asked for before the reply arrived; carried out on arrival.
    pub then: Option<SelectorUse>,
}

impl SelectorState {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn candidate(&self) -> Option<&SelectorCandidate> {
        let candidates = &self.reply.as_ref()?.candidates;
        candidates.get(self.chosen).or_else(|| candidates.first())
    }
}

pub fn ui(ui: &mut egui::Ui, state: &mut AppState) {
    let mut actions = Vec::new();
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.strong("Audio & Note Analyzer");
            if state.measurements.loading {
                ui.spinner();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        state.measurements.result.is_some(),
                        egui::Button::new(format!("{} Copy as code", icon::CLIPBOARD_TEXT)),
                    )
                    .on_hover_text(
                        "Copy ready-to-use MusicScript Track / Note code for the selection",
                    )
                    .clicked()
                {
                    actions.push(Action::CopyMeasurementCode);
                }
            });
        });
        ui.small("Click a Track or Note block in Studio View to analyze its audio & pitch metrics");
        ui.add_space(6.0);

        if !state.scene.topology_selections.is_empty() {
            selector_ui(ui, &mut state.selector, &mut actions);
            ui.separator();
        }

        if let Some(error) = &state.measurements.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
            return;
        }
        let Some(result) = &state.measurements.result else {
            super::empty_state(
                ui,
                icon::WAVE_SINE,
                "Select a Track in Tracks & Mixer or click a Note block in the Sequencer",
            );
            return;
        };

        for (index, entity) in result.entities.iter().enumerate() {
            if index > 0 {
                ui.separator();
            }
            entity_ui(ui, state, entity, index + 1);
        }
        for (index, object) in result.objects.iter().enumerate() {
            if index > 0 {
                ui.separator();
            }
            object_ui(ui, state, object, index + 1);
        }
        if let Some(pair) = &result.pair {
            ui.separator();
            pair_ui(ui, pair);
        }
        if result.entities.len() + result.objects.len() > 1
            && let Some(bounds) = &result.bounding_box
        {
            ui.separator();
            ui.strong("Selection bounds");
            egui::Grid::new("measurement_selection_bounds")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| bounds_rows(ui, bounds));
        }

        ui.separator();
        ui.weak(
            "Topology IDs belong to this render only. Rebuilding or remodeling the object may change them.",
        );
    });
    state.queue(actions);
}

/// The selector of the picked entities, with Copy and Insert.
pub fn selector_ui(ui: &mut egui::Ui, selector: &mut SelectorState, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        ui.label(format!("{} Selector", icon::SELECTION));
        if selector.loading {
            ui.spinner();
        }
    });
    if let Some(error) = &selector.error {
        ui.colored_label(ui.visuals().warn_fg_color, error);
        return;
    }
    let Some(reply) = &selector.reply else {
        return;
    };
    let Some(candidate) = selector.candidate().cloned() else {
        return;
    };
    ui.add(egui::Label::new(egui::RichText::new(&candidate.expression).monospace()).wrap());
    if !candidate.description.is_empty() {
        ui.weak(&candidate.description);
    }
    if candidate.index_based {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!(
                "{} No geometric selector matches exactly; this index may select another entity once the model changes",
                icon::WARNING
            ),
        );
    }
    ui.horizontal(|ui| {
        if ui
            .button(format!("{} Copy", icon::COPY))
            .on_hover_text("Copy the selector to the clipboard")
            .clicked()
        {
            actions.push(Action::CopySelector);
        }
        let insert = ui
            .button(format!("{} Insert", icon::CURSOR_TEXT))
            .on_hover_text("Insert the selector at the editor's cursor");
        if insert.clicked() {
            actions.push(Action::InsertSelector);
        }
        if reply.candidates.len() > 1 {
            let mut chosen = selector.chosen.min(reply.candidates.len() - 1);
            egui::ComboBox::from_id_salt("selector_alternatives")
                .selected_text("Alternatives")
                .show_ui(ui, |ui| {
                    for (index, alternative) in reply.candidates.iter().enumerate() {
                        ui.selectable_value(&mut chosen, index, &alternative.expression)
                            .on_hover_text(&alternative.description);
                    }
                });
            selector.chosen = chosen;
        }
    });
}

/// The measurement of `selection` in `result`.
pub fn measurement_of<'a>(
    result: &'a Measurement,
    selection: &TopologySelection,
) -> Option<&'a EntityMeasurement> {
    result.entities.iter().find(|entity| {
        entity.key == selection.node
            && entity.part == selection.part
            && entity.kind == selection.entity.topology_kind()
            && entity.identifier == selection.entity.identifier()
    })
}

fn entity_ui(ui: &mut egui::Ui, state: &AppState, entity: &EntityMeasurement, number: usize) {
    let name = state
        .scene
        .find(&entity.key)
        .map_or(entity.key.as_str(), |node| node.name.as_str());
    ui.strong(format!(
        "{number}. {} {} · {name}",
        kind_label(entity.kind),
        entity.identifier
    ));
    egui::Grid::new(("measurement_entity", number))
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            row(ui, "Note / Pitch", &title_case(&entity.geometry_type));
            row(ui, "Step / Pitch / Vel", &format_point(entity.center));
            if let Some(value) = entity.length {
                row(ui, "Duration (steps)", &format_number(value));
            }
            if let Some(value) = entity.area {
                row(ui, "Velocity (%)", &format_number(value));
            }
            if let Some(value) = entity.radius {
                row(ui, "Frequency (Hz)", &format_number(value));
            }
            if let Some(value) = entity.diameter {
                row(ui, "MIDI Note #", &format_number(value));
            }
            if let Some(bounds) = &entity.bounding_box {
                bounds_rows(ui, bounds);
            }
        });
}

fn object_ui(ui: &mut egui::Ui, state: &AppState, object: &ObjectMeasurement, number: usize) {
    let name = state
        .scene
        .find(&object.key)
        .map_or(object.key.as_str(), |node| node.name.as_str());
    ui.strong(format!("{number}. {name}"));
    egui::Grid::new(("measurement_object", number))
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            row(ui, "Instrument & Tempo", &object.shape_type);
            if let Some(value) = object.volume {
                row(ui, "Loop Duration (s)", &format_number(value));
            }
            if let Some(value) = object.length {
                row(ui, "Peak Level (dBFS)", &format_number(value));
            }
            if let Some(value) = object.area {
                row(ui, "RMS Level (dBFS)", &format_number(value));
            }
            row(ui, "Sub / Bass / Mid Energy", &format_point(object.center));
            row(
                ui,
                "Arrangement",
                &format!(
                    "{} bars · {} notes · {} steps · {} FX",
                    object.solids, object.faces, object.edges, object.vertices
                ),
            );
            if let Some(bounds) = &object.bounding_box {
                bounds_rows(ui, bounds);
            }
        });
}

/// Size, minimum and maximum of an exact bounding box.
fn bounds_rows(ui: &mut egui::Ui, bounds: &BoundingBox) {
    let [x, y, z] = bounds.size;
    row(
        ui,
        "Bounding box",
        &format!(
            "{} × {} × {}",
            format_number(x),
            format_number(y),
            format_number(z)
        ),
    );
    row(ui, "Min", &format_point(bounds.min));
    row(ui, "Max", &format_point(bounds.max));
}

fn pair_ui(ui: &mut egui::Ui, pair: &PairMeasurement) {
    ui.strong("Relationship");
    egui::Grid::new("measurement_pair")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            row(ui, "Distance", &format_number(pair.distance));
            row(ui, "Closest point 1", &format_point(pair.first_point));
            row(ui, "Closest point 2", &format_point(pair.second_point));
            if let Some(angle) = pair.angle_degrees {
                let label = match pair.angle_kind.as_deref() {
                    Some("edge_edge") => "Edge angle",
                    Some("face_face") => "Face angle",
                    Some("edge_face") => "Edge/plane angle",
                    _ => "Angle",
                };
                row(ui, label, &format!("{}°", format_number(angle)));
            }
        });
}

fn row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(label);
    ui.monospace(value);
    ui.end_row();
}

fn kind_label(kind: TopologyKind) -> &'static str {
    match kind {
        TopologyKind::Face => "Face",
        TopologyKind::Edge => "Edge",
        TopologyKind::Vertex => "Vertex",
    }
}

pub fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

pub fn format_point(point: [f64; 3]) -> String {
    format!(
        "({}, {}, {})",
        format_number(point[0]),
        format_number(point[1]),
        format_number(point[2])
    )
}

pub fn format_number(value: f64) -> String {
    let absolute = value.abs();
    if absolute != 0.0 && !(1e-5..1e9).contains(&absolute) {
        return format!("{value:.8e}");
    }
    let text = format!("{value:.9}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[cfg(test)]
mod tests {
    use super::{format_number, measurement_of};
    use crate::scene::{TopologyEntity, TopologySelection};
    use crate::worker::protocol::{EntityMeasurement, Measurement, TopologyIdentity, TopologyKind};

    #[test]
    fn the_property_sheet_finds_the_measurement_of_its_selection() {
        let entity = |kind, identifier| EntityMeasurement {
            key: "s1-n1".into(),
            part: 0,
            kind,
            identifier,
            geometry_type: "plane".into(),
            center: [0.0; 3],
            position: None,
            length: None,
            area: Some(4.0),
            radius: None,
            diameter: None,
            bounding_box: None,
        };
        let result = Measurement {
            id: 1,
            identity: TopologyIdentity {
                scope: "render".into(),
                persistent: false,
            },
            entities: vec![entity(TopologyKind::Face, 6), entity(TopologyKind::Edge, 6)],
            pair: None,
            objects: Vec::new(),
            bounding_box: None,
        };
        let selection = |entity| TopologySelection {
            node: "s1-n1".into(),
            part: 0,
            entity,
            position: Default::default(),
        };

        let edge = measurement_of(&result, &selection(TopologyEntity::Edge(6)));
        assert_eq!(edge.map(|e| e.kind), Some(TopologyKind::Edge));
        assert!(measurement_of(&result, &selection(TopologyEntity::Face(7))).is_none());
    }

    #[test]
    fn numeric_display_keeps_precision_without_noise() {
        assert_eq!(format_number(5.0), "5");
        assert_eq!(format_number(1.2345678912), "1.234567891");
        assert_eq!(format_number(0.000_001), "1.00000000e-6");
    }
}
