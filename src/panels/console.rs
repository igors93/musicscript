//! Interactive Python console running in the worker.
//!
//! Its namespace has the script API (`show_object` adds to the view) and the
//! variables of the last run. Enter runs the input, Shift+Enter adds a line,
//! Up/Down browse the history. Results with an image (a matplotlib figure,
//! anything with `_repr_png_`) show it, decoded before it gets here.

use egui::{Color32, Key, Modifiers, RichText};
use egui_phosphor::regular as icon;

use crate::app::AppState;
use crate::commands::Action;
use crate::worker::protocol::{DisplayImage, DisplayOutput};

const MAX_ENTRIES: usize = 2000;
/// Images kept; older ones are replaced by a note, to bound memory.
const MAX_IMAGES: usize = 50;
/// Decoded pixels retained on the CPU. Textures use additional GPU memory.
const MAX_IMAGE_MEMORY: usize = 128 * 1024 * 1024;
/// Images are drawn at most this tall, in points.
const MAX_IMAGE_HEIGHT: f32 = 360.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Input,
    Output,
    Error,
    Info,
}

pub struct Entry {
    pub kind: EntryKind,
    pub text: String,
    /// A rich result's picture, shown above its text.
    pub image: Option<ConsoleImage>,
}

/// An image in the console; its texture is made when it is first drawn.
pub struct ConsoleImage {
    pixels: DisplayImage,
    texture: Option<egui::TextureHandle>,
}

#[derive(Default)]
pub struct ConsoleState {
    pub entries: Vec<Entry>,
    pub input: String,
    /// The last statement is incomplete (a block is open).
    pub more: bool,
    pub busy: bool,
    history: Vec<String>,
    history_index: Option<usize>,
    focus_input: bool,
}

impl ConsoleState {
    /// Applies completion results for the input: a unique match or the
    /// common prefix is inserted, otherwise the candidates are listed.
    pub fn complete(&mut self, inserts: &[(String, String)], signatures: &[String]) {
        if inserts.is_empty() {
            if !signatures.is_empty() {
                self.push(EntryKind::Info, &(signatures.join("\n") + "\n"));
            }
            return;
        }
        let prefix = common_prefix(inserts.iter().map(|(_, insert)| insert.as_str()));
        if !prefix.is_empty() {
            self.input.push_str(prefix);
        } else if inserts.len() > 1 {
            let names: Vec<&str> = inserts.iter().map(|(name, _)| name.as_str()).collect();
            self.push(EntryKind::Info, &(names.join("  ") + "\n"));
        }
        self.focus_input = true;
    }

    pub fn push(&mut self, kind: EntryKind, text: &str) {
        // Consecutive output chunks are merged into one entry.
        if let Some(last) = self.entries.last_mut()
            && last.kind == kind
            && kind != EntryKind::Input
            && last.image.is_none()
            && !last.text.ends_with('\n')
        {
            last.text.push_str(text);
            return;
        }
        self.add(Entry {
            kind,
            text: text.to_owned(),
            image: None,
        });
    }

    /// A rich result of a command: its image, if any, then its text.
    pub fn push_display(&mut self, output: DisplayOutput) {
        let text = if output.text.is_empty() {
            String::new()
        } else {
            output.text + "\n"
        };
        match output.image {
            Some(pixels) => {
                self.add(Entry {
                    kind: EntryKind::Output,
                    text,
                    image: Some(ConsoleImage {
                        pixels,
                        texture: None,
                    }),
                });
                self.trim_images(MAX_IMAGES, MAX_IMAGE_MEMORY);
            }
            None if !text.is_empty() => self.push(EntryKind::Output, &text),
            None => {}
        }
    }

    fn add(&mut self, entry: Entry) {
        self.entries.push(entry);
        if self.entries.len() > MAX_ENTRIES {
            self.entries.drain(..self.entries.len() - MAX_ENTRIES);
        }
    }

    fn trim_images(&mut self, max_images: usize, max_bytes: usize) {
        let mut images = self
            .entries
            .iter()
            .filter(|entry| entry.image.is_some())
            .count();
        let mut bytes: usize = self
            .entries
            .iter()
            .filter_map(|entry| entry.image.as_ref())
            .map(|image| image.pixels.rgba.len())
            .sum();
        for entry in &mut self.entries {
            if images <= max_images && bytes <= max_bytes {
                break;
            }
            if let Some(image) = entry.image.take() {
                entry.text.insert_str(0, "[image]\n");
                images -= 1;
                bytes = bytes.saturating_sub(image.pixels.rgba.len());
            }
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn record_input(&mut self, source: &str) {
        let prompt = if self.more { "... " } else { ">>> " };
        let shown: Vec<String> = source
            .lines()
            .enumerate()
            .map(|(i, line)| format!("{}{line}", if i == 0 { prompt } else { "... " }))
            .collect();
        self.push(EntryKind::Input, &(shown.join("\n") + "\n"));
        if !source.trim().is_empty() && self.history.last().map(String::as_str) != Some(source) {
            self.history.push(source.to_owned());
        }
        self.history_index = None;
        self.busy = true;
    }
}

fn common_prefix<'a>(mut words: impl Iterator<Item = &'a str>) -> &'a str {
    let Some(first) = words.next() else { return "" };
    let mut length = first.len();
    for word in words {
        length = first
            .char_indices()
            .zip(word.chars())
            .take_while(|((_, a), b)| a == b)
            .last()
            .map_or(0, |((i, c), _)| i + c.len_utf8())
            .min(length);
    }
    &first[..length]
}

pub fn ui(ui: &mut egui::Ui, state: &mut AppState) {
    let mut actions = Vec::new();
    let colors = Colors::new(ui.visuals());
    let worker_ready = state.worker.is_ready();
    let console = &mut state.console;

    egui::Panel::bottom("console_input")
        .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(6, 4)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(if console.more { "..." } else { ">>>" })
                        .monospace()
                        .color(colors.prompt),
                );
                let input_id = ui.id().with("console_input_text");
                let focused = ui.memory(|m| m.has_focus(input_id));
                if focused {
                    handle_keys(ui, console, &mut actions);
                }
                let response = ui.add_enabled(
                    worker_ready,
                    egui::TextEdit::multiline(&mut console.input)
                        .id(input_id)
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(1)
                        // Room for the buttons, Stop included while busy.
                        .desired_width(
                            ui.available_width() - if console.busy { 92.0 } else { 62.0 },
                        )
                        .hint_text(if worker_ready {
                            "Python — Enter to run"
                        } else {
                            "Waiting for Python…"
                        }),
                );
                if std::mem::take(&mut console.focus_input) {
                    response.request_focus();
                }
                if console.busy
                    && ui
                        .button(icon::STOP)
                        .on_hover_text(
                            "Stop the running command (Ctrl+Shift+F5); its variables stay",
                        )
                        .clicked()
                {
                    actions.push(Action::Stop);
                }
                if ui
                    .button(icon::BROOM)
                    .on_hover_text("Clear the console")
                    .clicked()
                {
                    actions.push(Action::ClearConsole);
                }
                if ui
                    .button(icon::ARROW_COUNTER_CLOCKWISE)
                    .on_hover_text("Restart the console (forgets its variables)")
                    .clicked()
                {
                    actions.push(Action::ResetConsole);
                }
            });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            output(ui, console, &colors);
        });

    state.queue(actions);
}

fn output(ui: &mut egui::Ui, console: &mut ConsoleState, colors: &Colors) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    if console.entries.is_empty() {
                        ui.weak(
                            "Python console. Try: show_object(cq.Workplane().sphere(5)) · \
                             selected() returns what is picked in the 3D view",
                        );
                    }
                    for entry in &mut console.entries {
                        if let Some(image) = &mut entry.image {
                            let texture = image.texture.get_or_insert_with(|| {
                                ui.ctx().load_texture(
                                    "console image",
                                    egui::ColorImage::from_rgba_unmultiplied(
                                        image.pixels.size,
                                        &image.pixels.rgba,
                                    ),
                                    egui::TextureOptions::LINEAR,
                                )
                            });
                            let size = texture.size_vec2();
                            let scale = (ui.available_width() / size.x)
                                .min(MAX_IMAGE_HEIGHT / size.y)
                                .min(1.0);
                            ui.add_space(4.0);
                            ui.image((texture.id(), size * scale));
                            ui.add_space(4.0);
                        }
                        if entry.text.is_empty() {
                            continue;
                        }
                        let color = match entry.kind {
                            EntryKind::Input => colors.input,
                            EntryKind::Output => colors.output,
                            EntryKind::Error => colors.error,
                            EntryKind::Info => colors.info,
                        };
                        let text = entry.text.strip_suffix('\n').unwrap_or(&entry.text);
                        ui.add(
                            egui::Label::new(RichText::new(text).monospace().color(color))
                                .selectable(true),
                        );
                    }
                    if console.busy {
                        ui.spinner();
                    }
                });
        });
}

fn handle_keys(ui: &egui::Ui, console: &mut ConsoleState, actions: &mut Vec<Action>) {
    let submit = ui.input_mut(|i| {
        // Shift+Enter falls through to the text edit and inserts a newline.
        !i.modifiers.shift && i.consume_key(Modifiers::NONE, Key::Enter)
    });
    if submit {
        let source = std::mem::take(&mut console.input);
        actions.push(Action::ConsoleSubmit(source));
        console.focus_input = true;
        return;
    }
    if !console.input.is_empty() && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab)) {
        actions.push(Action::ConsoleComplete);
        return;
    }
    // History navigation only makes sense for single-line input.
    if console.input.contains('\n') || console.history.is_empty() {
        return;
    }
    if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) {
        let index = console
            .history_index
            .map_or(console.history.len() - 1, |i| i.saturating_sub(1));
        console.history_index = Some(index);
        console.input = console.history[index].clone();
    } else if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown)) {
        match console.history_index {
            Some(i) if i + 1 < console.history.len() => {
                console.history_index = Some(i + 1);
                console.input = console.history[i + 1].clone();
            }
            _ => {
                console.history_index = None;
                console.input.clear();
            }
        }
    }
}

struct Colors {
    prompt: Color32,
    input: Color32,
    output: Color32,
    error: Color32,
    info: Color32,
}

impl Colors {
    fn new(visuals: &egui::Visuals) -> Self {
        let text = visuals.text_color();
        Self {
            prompt: visuals.hyperlink_color,
            input: visuals.hyperlink_color.lerp_to_gamma(text, 0.35),
            output: text,
            error: visuals.error_fg_color,
            info: visuals.weak_text_color(),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/rust/panels/test_console.rs"]
mod tests;
