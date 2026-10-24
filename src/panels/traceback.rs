//! The error of the last run. Clicking a frame of the script jumps to its line.

use egui::RichText;
use egui_extras::{Column, TableBuilder};
use egui_phosphor::regular as icon;

use crate::app::AppState;
use crate::commands::Action;
use crate::worker::protocol::SCRIPT_FILENAME;

pub fn ui(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(error) = &state.traceback else {
        super::empty_state(ui, icon::CHECK_CIRCLE, "No errors");
        return;
    };
    let mut actions = Vec::new();
    let script_name = state.document.display_name();

    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let error_color = ui.visuals().error_fg_color;
                ui.label(RichText::new(icon::WARNING_CIRCLE).color(error_color));
                let summary = format!("{}: {}", error.type_name, error.message);
                ui.add(
                    egui::Label::new(RichText::new(&summary).color(error_color).strong())
                        .truncate(),
                )
                .on_hover_text(&summary);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(icon::COPY)
                        .on_hover_text("Copy the full traceback")
                        .clicked()
                    {
                        ui.ctx().copy_text(error.text.clone());
                    }
                });
            });
        });

    let row_height = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .auto_shrink([false, false])
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(140.0).at_least(60.0).clip(true))
        .column(Column::initial(50.0).at_least(40.0))
        .column(Column::initial(110.0).at_least(50.0).clip(true))
        .column(Column::remainder().clip(true))
        .header(row_height, |mut header| {
            for title in ["File", "Line", "Function", "Code"] {
                header.col(|ui| {
                    ui.strong(title);
                });
            }
        })
        .body(|body| {
            body.rows(row_height, error.frames.len(), |mut row| {
                let frame = &error.frames[row.index()];
                let in_script = frame.file == SCRIPT_FILENAME;
                let file = if in_script {
                    script_name.as_str()
                } else {
                    frame.file.as_str()
                };
                // Not selectable: a selectable label takes the clicks on
                // its text away from the row, which jumps to the line.
                let cell = |text: RichText| egui::Label::new(text).selectable(false);
                row.col(|ui| {
                    if in_script {
                        ui.add(cell(RichText::new(file).strong()));
                    } else {
                        ui.add(cell(RichText::new(file).weak()))
                            .on_hover_text(&frame.file);
                    }
                });
                row.col(|ui| {
                    ui.add(cell(RichText::new(frame.line.to_string())));
                });
                row.col(|ui| {
                    ui.add(cell(RichText::new(&frame.function).weak()));
                });
                row.col(|ui| {
                    ui.add(cell(RichText::new(&frame.code).monospace()).truncate());
                });
                if in_script && row.response().clicked() {
                    actions.push(Action::GoToLine(frame.line));
                }
            });
        });
    state.queue(actions);
}
