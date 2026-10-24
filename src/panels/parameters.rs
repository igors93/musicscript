//! The script's top-level numbers as spin boxes and sliders. A change
//! rewrites the literal in the script and renders again: the script stays
//! the source of truth, and undo works as for any edit.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::RangeInclusive;

use egui_phosphor::regular as icon;

use crate::app::AppState;
use crate::commands::Action;
use crate::parameters::{self, Parameter};

#[derive(Default)]
pub struct ParametersState {
    /// The parameters of the text with this hash.
    parsed: Option<(u64, Vec<Parameter>)>,
    /// Slider ranges of parameters without a commented range, chosen from
    /// their first value so that the slider does not move under a drag.
    ranges: HashMap<String, RangeInclusive<f64>>,
}

impl ParametersState {
    /// The parameters of `text`, parsed once per version of the text.
    pub fn of(&mut self, text: &str) -> &[Parameter] {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();
        if self
            .parsed
            .as_ref()
            .is_none_or(|(parsed, _)| *parsed != hash)
        {
            self.parsed = Some((hash, parameters::parameters(text)));
        }
        &self.parsed.as_ref().expect("parsed above").1
    }

    /// The slider range of `parameter`: its commented range, or one around
    /// its first value, widened to keep the current value inside.
    pub fn slider_range(&mut self, parameter: &Parameter) -> RangeInclusive<f64> {
        if let Some(range) = &parameter.range {
            return range.clone();
        }
        let value = parameter.value;
        let range = self
            .ranges
            .entry(parameter.name.clone())
            .or_insert_with(|| automatic_range(value, parameter.integer));
        if !range.contains(&value) {
            *range = range.start().min(value)..=range.end().max(value);
        }
        range.clone()
    }
}

/// Zero to twice the value (or twice the value to zero), at least 10 wide.
fn automatic_range(value: f64, integer: bool) -> RangeInclusive<f64> {
    let span = (2.0 * value.abs()).max(10.0);
    let span = if integer { span.ceil() } else { span };
    if value < 0.0 {
        -span..=0.0
    } else if value == 0.0 {
        -span / 2.0..=span / 2.0
    } else {
        0.0..=span
    }
}

pub fn ui(ui: &mut egui::Ui, state: &mut AppState) {
    let mut actions = Vec::new();
    let editable = !state.is_debugging();
    let found = state.parameters.of(&state.document.text).to_vec();

    if found.is_empty() {
        super::empty_state(
            ui,
            icon::SLIDERS_HORIZONTAL,
            "Top-level script parameters (e.g. bpm = 140 # [80, 180]) appear here as live sliders",
        );
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_enabled_ui(editable, |ui| {
                egui::Grid::new("parameters_grid")
                    .num_columns(3)
                    .spacing([10.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for parameter in &found {
                            row(ui, state, parameter, &mut actions);
                        }
                    });
            });
            ui.add_space(6.0);
            if editable {
                ui.weak(
                    "A change rewrites the number in the script and renders again. \
                     A comment such as # [10, 100] or # [0, 5, 0.5] sets the slider's \
                     range and step.",
                );
            } else {
                ui.weak("Parameters can be changed once debugging ends.");
            }
        });
    state.queue(actions);
}

fn row(ui: &mut egui::Ui, state: &mut AppState, parameter: &Parameter, actions: &mut Vec<Action>) {
    let name = ui
        .link(&parameter.name)
        .on_hover_text(format!("Line {}: go to it", parameter.line));
    if name.clicked() {
        actions.push(Action::GoToLine(parameter.line));
    }

    let precision = parameter.precision();
    let mut value = parameter.value;
    let mut drag = egui::DragValue::new(&mut value)
        .speed(parameter.drag_speed())
        .min_decimals(precision)
        .max_decimals(precision);
    if let Some(range) = &parameter.range {
        drag = drag.range(range.clone());
    }
    let mut changed = ui.add(drag).changed();

    let range = state.parameters.slider_range(parameter);
    let mut slider = egui::Slider::new(&mut value, range).show_value(false);
    if parameter.integer {
        slider = slider.integer();
    } else if let Some(step) = parameter.step {
        slider = slider.step_by(step);
    }
    changed |= ui.add(slider).changed();
    ui.end_row();

    if changed && value != parameter.value {
        actions.push(Action::SetParameter {
            name: parameter.name.clone(),
            line: parameter.line,
            value,
        });
    }
}

#[cfg(test)]
#[path = "../../tests/rust/panels/test_parameters.rs"]
mod tests;
