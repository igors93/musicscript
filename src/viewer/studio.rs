//! Interactive DAW Studio View: Transport Bar, Step Sequencer / Piano Roll,
//! Multi-Track Waveform & Live Oscilloscope, and 16-Band Spectrum Analyzer.

use egui::{
    Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use egui_phosphor::regular as icon;
use glam::Vec3;

use crate::audio::AudioPlayer;
use crate::commands::Action;
use crate::scene::{Scene, SceneNode, TopologyEntity, TopologySelection};
use crate::viewer::ViewerEvent;
use crate::worker::protocol::ExportFormat;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StudioViewMode {
    #[default]
    StudioDaw,
    Sequencer,
    WaveformScope,
    SpectrumAnalyzer,
    Visualizer3D,
}

impl StudioViewMode {
    pub const ALL: [Self; 5] = [
        Self::StudioDaw,
        Self::Sequencer,
        Self::WaveformScope,
        Self::SpectrumAnalyzer,
        Self::Visualizer3D,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::StudioDaw => "Studio DAW",
            Self::Sequencer => "Sequencer",
            Self::WaveformScope => "Waveform",
            Self::SpectrumAnalyzer => "Spectrum",
            Self::Visualizer3D => "3D View",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::StudioDaw => icon::SLIDERS_HORIZONTAL,
            Self::Sequencer => icon::MUSIC_NOTES,
            Self::WaveformScope => icon::WAVE_SINE,
            Self::SpectrumAnalyzer => icon::CHART_BAR,
            Self::Visualizer3D => icon::CUBE,
        }
    }
}

const FREQ_BAND_LABELS: [&str; 16] = [
    "32", "50", "80", "120", "180", "280", "450", "700", "1.1k", "1.7k", "2.6k", "4k", "6.3k",
    "9.5k", "12k", "16k",
];

/// Render the top transport & view mode bar inside the central Studio View.
pub fn draw_transport_bar(
    ui: &mut Ui,
    rect: Rect,
    scene: &Scene,
    audio: &AudioPlayer,
    mode: &mut StudioViewMode,
    events: &mut Vec<ViewerEvent>,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(20, 22, 28));
    painter.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, Color32::from_rgb(44, 48, 60)),
    );

    let inner = rect.shrink2(Vec2::new(10.0, 5.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));

    child.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;

        let is_playing = audio.is_playing();
        let play_label = if is_playing {
            format!("{} Pause", icon::PAUSE)
        } else {
            format!("{} Play", icon::PLAY)
        };
        let play_fill = if is_playing {
            Color32::from_rgb(235, 145, 38)
        } else {
            Color32::from_rgb(46, 178, 106)
        };
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(play_label)
                        .color(Color32::WHITE)
                        .strong(),
                )
                .fill(play_fill)
                .min_size(Vec2::new(76.0, 28.0)),
            )
            .on_hover_text("Play / Pause beat audio (Space)")
            .clicked()
        {
            if scene.nodes.is_empty() {
                events.push(ViewerEvent::Command(Action::Render));
            } else {
                audio.toggle_play();
            }
        }

        if ui
            .add(
                egui::Button::new(egui::RichText::new(icon::STOP).color(Color32::WHITE))
                    .fill(Color32::from_rgb(48, 52, 66))
                    .min_size(Vec2::new(30.0, 28.0)),
            )
            .on_hover_text("Stop & return playhead to start")
            .clicked()
        {
            audio.stop();
        }

        let looping = audio.is_looping();
        let loop_fill = if looping {
            Color32::from_rgb(56, 118, 220)
        } else {
            Color32::from_rgb(42, 46, 58)
        };
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(format!("{} Loop", icon::REPEAT)).color(Color32::WHITE),
                )
                .fill(loop_fill)
                .min_size(Vec2::new(62.0, 28.0)),
            )
            .on_hover_text("Loop playback continuously")
            .clicked()
        {
            audio.set_looping(!looping);
        }

        ui.separator();

        // Digital Studio LCD Readout (BPM | Bar.Beat.Step | Time)
        let bpm = audio.bpm();
        let cur_sec = audio.playhead_seconds();
        let dur_sec = audio.duration_seconds();
        let step_pos = audio.current_step();
        let bar_num = (step_pos as usize) / 16 + 1;
        let beat_num = ((step_pos as usize) % 16) / 4 + 1;
        let sub_step = ((step_pos as usize) % 4) + 1;

        let lcd_text = format!(
            "{bpm:>5.1} BPM   │   BAR {bar_num}:{beat_num}:{sub_step}   │   {cur_sec:04.1}s / {dur_sec:04.1}s"
        );
        egui::Frame::new()
            .fill(Color32::from_rgb(12, 14, 18))
            .stroke(Stroke::new(1.0, Color32::from_rgb(52, 62, 82)))
            .corner_radius(CornerRadius::same(4))
            .inner_margin(egui::Margin::symmetric(10, 4))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(lcd_text)
                        .monospace()
                        .size(12.5)
                        .color(Color32::from_rgb(110, 235, 175)),
                );
            });

        ui.separator();

        // Master Volume + Live Stereo VU Meter
        ui.label(
            egui::RichText::new(icon::SPEAKER_HIGH)
                .size(14.0)
                .color(Color32::from_rgb(180, 190, 210)),
        );
        let mut master_vol = audio.master_volume();
        if ui
            .add_sized(
                [72.0, 20.0],
                egui::Slider::new(&mut master_vol, 0.0..=1.25).show_value(false),
            )
            .on_hover_text(format!("Master Volume: {:.0}%", master_vol * 100.0))
            .changed()
        {
            audio.set_master_volume(master_vol);
        }

        let [vu_l, vu_r] = audio.stereo_vu();
        let (vu_rect, _) = ui.allocate_exact_size(Vec2::new(56.0, 18.0), Sense::hover());
        draw_mini_stereo_vu(ui.painter(), vu_rect, vu_l, vu_r);

        ui.separator();

        // Quick Export buttons (WAV / MIDI)
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(format!("{} WAV", icon::DOWNLOAD_SIMPLE)).size(12.0),
                )
                .fill(Color32::from_rgb(38, 44, 58)),
            )
            .on_hover_text("Export Master Mix to .wav audio file")
            .clicked()
        {
            events.push(ViewerEvent::Command(Action::Export(ExportFormat::Wav)));
        }
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(format!("{} MIDI", icon::PIANO_KEYS)).size(12.0),
                )
                .fill(Color32::from_rgb(38, 44, 58)),
            )
            .on_hover_text("Export multi-track MIDI (.mid) file")
            .clicked()
        {
            events.push(ViewerEvent::Command(Action::Export(ExportFormat::Midi)));
        }

        // Right-aligned View Mode Tabs
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for candidate in StudioViewMode::ALL.into_iter().rev() {
                let selected = *mode == candidate;
                let fill = if selected {
                    Color32::from_rgb(68, 132, 245)
                } else {
                    Color32::from_rgb(34, 38, 48)
                };
                let text_col = if selected {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(185, 192, 208)
                };
                let label = format!("{} {}", candidate.icon(), candidate.label());
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new(label).size(12.0).color(text_col))
                            .fill(fill)
                            .min_size(Vec2::new(72.0, 26.0)),
                    )
                    .clicked()
                {
                    *mode = candidate;
                }
            }
        });
    });

    if audio.is_playing() {
        ui.ctx().request_repaint();
    }
}

fn draw_mini_stereo_vu(painter: &egui::Painter, rect: Rect, vu_l: f32, vu_r: f32) {
    painter.rect_filled(rect, 2.0, Color32::from_rgb(12, 14, 18));
    let h = (rect.height() - 5.0) * 0.5;
    let l_rect = Rect::from_min_size(
        Pos2::new(rect.left() + 2.0, rect.top() + 2.0),
        Vec2::new(rect.width() - 4.0, h),
    );
    let r_rect = Rect::from_min_size(
        Pos2::new(rect.left() + 2.0, rect.top() + 3.0 + h),
        Vec2::new(rect.width() - 4.0, h),
    );
    for (bar, val) in [(l_rect, vu_l), (r_rect, vu_r)] {
        let frac = val.clamp(0.0, 1.0);
        let fill_w = bar.width() * frac;
        let col = if frac > 0.90 {
            Color32::from_rgb(245, 72, 72)
        } else if frac > 0.70 {
            Color32::from_rgb(245, 196, 56)
        } else {
            Color32::from_rgb(56, 222, 128)
        };
        painter.rect_filled(
            Rect::from_min_size(bar.min, Vec2::new(fill_w, bar.height())),
            1.0,
            col,
        );
    }
}

/// Draw the 2D Studio View modes (`StudioDaw`, `Sequencer`, `WaveformScope`, `SpectrumAnalyzer`).
pub fn draw_studio_canvas(
    ui: &mut Ui,
    rect: Rect,
    scene: &Scene,
    audio: &AudioPlayer,
    mode: StudioViewMode,
    busy: Option<&str>,
    events: &mut Vec<ViewerEvent>,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(15, 17, 23));

    if scene.nodes.is_empty() {
        draw_empty_studio_state(&painter, rect, busy);
        return;
    }

    match mode {
        StudioViewMode::StudioDaw => {
            let split_y = (rect.top() + rect.height() * 0.64).clamp(
                rect.top() + 140.0,
                rect.bottom() - 130.0,
            );
            let seq_rect = Rect::from_min_max(rect.min, Pos2::new(rect.right(), split_y));
            let bottom_rect =
                Rect::from_min_max(Pos2::new(rect.left(), split_y + 2.0), rect.max);

            draw_sequencer_view(ui, seq_rect, scene, audio, events);

            painter.line_segment(
                [
                    Pos2::new(rect.left(), split_y + 1.0),
                    Pos2::new(rect.right(), split_y + 1.0),
                ],
                Stroke::new(2.0, Color32::from_rgb(38, 42, 54)),
            );

            let mid_x = bottom_rect.left() + bottom_rect.width() * 0.56;
            let wave_rect =
                Rect::from_min_max(bottom_rect.min, Pos2::new(mid_x - 1.0, bottom_rect.bottom()));
            let spec_rect =
                Rect::from_min_max(Pos2::new(mid_x + 1.0, bottom_rect.top()), bottom_rect.max);

            draw_waveform_and_scope(ui, wave_rect, scene, audio, false);
            draw_spectrum_analyzer(ui, spec_rect, scene, audio, false);
        }
        StudioViewMode::Sequencer => {
            draw_sequencer_view(ui, rect, scene, audio, events);
        }
        StudioViewMode::WaveformScope => {
            draw_waveform_and_scope(ui, rect, scene, audio, true);
        }
        StudioViewMode::SpectrumAnalyzer => {
            draw_spectrum_analyzer(ui, rect, scene, audio, true);
        }
        StudioViewMode::Visualizer3D => {}
    }
}

fn draw_empty_studio_state(painter: &egui::Painter, rect: Rect, busy: Option<&str>) {
    let center = rect.center();
    let title = if let Some(msg) = busy {
        format!("Synthesizing Beat ({msg})…")
    } else {
        "MusicScript Beat Studio".to_owned()
    };
    let subtitle = "Press F5 (or click Run) to synthesize your script, or open a beat from File → Examples.";
    painter.text(
        Pos2::new(center.x, center.y - 14.0),
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(20.0),
        Color32::from_rgb(210, 218, 235),
    );
    painter.text(
        Pos2::new(center.x, center.y + 16.0),
        Align2::CENTER_CENTER,
        subtitle,
        FontId::proportional(13.5),
        Color32::from_rgb(135, 145, 168),
    );
}

fn draw_sequencer_view(
    ui: &mut Ui,
    rect: Rect,
    scene: &Scene,
    audio: &AudioPlayer,
    events: &mut Vec<ViewerEvent>,
) {
    let painter = ui.painter_at(rect);
    let header_w = 196.0_f32.min(rect.width() * 0.32);
    let ruler_h = 26.0_f32;
    let grid_left = rect.left() + header_w;
    let grid_width = (rect.width() - header_w - 8.0).max(60.0);

    let total_steps = audio.total_steps().max(16);
    let step_w = grid_width / (total_steps as f32);

    // Top Ruler Bar
    let ruler_rect = Rect::from_min_max(
        rect.min,
        Pos2::new(rect.right(), rect.top() + ruler_h),
    );
    painter.rect_filled(ruler_rect, 0.0, Color32::from_rgb(24, 27, 35));
    painter.text(
        Pos2::new(rect.left() + 10.0, ruler_rect.center().y),
        Align2::LEFT_CENTER,
        "TRACKS / SEQUENCER",
        FontId::proportional(11.5),
        Color32::from_rgb(145, 155, 178),
    );

    // Interactive timeline seek on ruler click/drag
    let timeline_rect = Rect::from_min_max(
        Pos2::new(grid_left, rect.top()),
        Pos2::new(grid_left + grid_width, rect.top() + ruler_h),
    );
    let ruler_resp = ui.allocate_rect(timeline_rect, Sense::click_and_drag());
    if (ruler_resp.clicked() || ruler_resp.dragged())
        && let Some(pos) = ruler_resp.interact_pointer_pos()
    {
        let frac = ((pos.x - grid_left) / grid_width).clamp(0.0, 0.999);
        audio.seek_fraction(frac);
    }

    // Draw Bar & Beat markers on ruler
    for s in 0..=total_steps {
        let x = grid_left + (s as f32) * step_w;
        if s % 16 == 0 && s < total_steps {
            painter.line_segment(
                [Pos2::new(x, ruler_rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.2, Color32::from_rgb(56, 62, 78)),
            );
            painter.text(
                Pos2::new(x + 5.0, ruler_rect.center().y),
                Align2::LEFT_CENTER,
                format!("Bar {}", s / 16 + 1),
                FontId::monospace(11.0),
                Color32::from_rgb(190, 200, 222),
            );
        } else if s % 4 == 0 && s < total_steps {
            painter.line_segment(
                [
                    Pos2::new(x, ruler_rect.bottom() - 6.0),
                    Pos2::new(x, rect.bottom()),
                ],
                Stroke::new(1.0, Color32::from_rgb(34, 38, 48)),
            );
            painter.text(
                Pos2::new(x + 3.0, ruler_rect.center().y + 1.0),
                Align2::LEFT_CENTER,
                format!("{}.{}", s / 16 + 1, (s % 16) / 4 + 1),
                FontId::monospace(9.5),
                Color32::from_rgb(112, 120, 140),
            );
        } else if step_w >= 10.0 && s < total_steps {
            painter.line_segment(
                [
                    Pos2::new(x, ruler_rect.bottom()),
                    Pos2::new(x, rect.bottom()),
                ],
                Stroke::new(0.5, Color32::from_rgb(24, 27, 35)),
            );
        }
    }

    painter.line_segment(
        [ruler_rect.left_bottom(), ruler_rect.right_bottom()],
        Stroke::new(1.0, Color32::from_rgb(44, 48, 62)),
    );

    let body_top = ruler_rect.bottom();
    let body_h = (rect.bottom() - body_top).max(40.0);
    let n_tracks = scene.nodes.len().max(1);
    let row_h = (body_h / (n_tracks as f32)).clamp(38.0, 76.0);

    let playhead_step = audio.current_step();
    let is_playing = audio.is_playing();
    let any_solo = scene.nodes.iter().any(SceneNode::is_solo);

    for (idx, node) in scene.nodes.iter().enumerate() {
        let y0 = body_top + (idx as f32) * row_h;
        if y0 >= rect.bottom() - 8.0 {
            break;
        }
        let y1 = (y0 + row_h).min(rect.bottom());
        let row_rect = Rect::from_min_max(Pos2::new(rect.left(), y0), Pos2::new(rect.right(), y1));
        let is_selected = scene.is_selected(&node.key);
        let is_active = if any_solo {
            node.is_solo()
        } else {
            !node.is_muted()
        };

        let [r, g, b] = node
            .overrides
            .color
            .or(node.color)
            .unwrap_or([0.35, 0.72, 0.95]);
        let track_color = Color32::from_rgb(
            (r * 255.0) as u8,
            (g * 255.0) as u8,
            (b * 255.0) as u8,
        );

        // Row background
        let bg_col = if is_selected {
            Color32::from_rgb(28, 34, 48)
        } else if idx % 2 == 0 {
            Color32::from_rgb(17, 19, 26)
        } else {
            Color32::from_rgb(20, 22, 30)
        };
        painter.rect_filled(row_rect, 0.0, bg_col);

        // Left Track Header
        let hdr_rect = Rect::from_min_max(
            Pos2::new(rect.left(), y0),
            Pos2::new(grid_left, y1),
        );
        let hdr_resp = ui.allocate_rect(hdr_rect, Sense::click());
        if hdr_resp.clicked() {
            let extend = ui.input(|i| i.modifiers.command);
            events.push(ViewerEvent::Picked {
                key: Some(node.key.clone()),
                topology: None,
                extend,
            });
        }

        // Left color stripe
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(hdr_rect.left(), y0 + 2.0),
                Pos2::new(hdr_rect.left() + 5.0, y1 - 2.0),
            ),
            2.0,
            if is_active {
                track_color
            } else {
                Color32::from_rgb(75, 80, 92)
            },
        );

        let name_col = if !is_active {
            Color32::from_rgb(110, 115, 130)
        } else if is_selected {
            Color32::WHITE
        } else {
            Color32::from_rgb(225, 230, 242)
        };
        painter.text(
            Pos2::new(hdr_rect.left() + 12.0, y0 + row_h * 0.34),
            Align2::LEFT_CENTER,
            &node.name,
            FontId::proportional(13.0),
            name_col,
        );

        let sub_badge = format!(
            "[{}] · {} · Vol {:.0}%",
            if node.instrument.is_empty() {
                "synth"
            } else {
                &node.instrument
            },
            if node.fx_summary.is_empty() {
                "Dry"
            } else {
                &node.fx_summary
            },
            node.effective_volume() * 100.0
        );
        painter.text(
            Pos2::new(hdr_rect.left() + 12.0, y0 + row_h * 0.72),
            Align2::LEFT_CENTER,
            sub_badge,
            FontId::monospace(10.0),
            Color32::from_rgb(130, 140, 165),
        );

        // Live Track Mini VU bar inside header right edge
        let tr_vu = audio.track_vu(&node.key).clamp(0.0, 1.0);
        let vu_bar_h = (row_h - 10.0).max(8.0);
        let vu_bg = Rect::from_min_size(
            Pos2::new(hdr_rect.right() - 10.0, y0 + 5.0),
            Vec2::new(4.0, vu_bar_h),
        );
        painter.rect_filled(vu_bg, 1.0, Color32::from_rgb(10, 12, 16));
        if tr_vu > 0.01 {
            let active_h = vu_bar_h * tr_vu;
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(vu_bg.left(), vu_bg.bottom() - active_h),
                    vu_bg.max,
                ),
                1.0,
                track_color,
            );
        }

        // Pitch range on track for vertical Piano Roll placement within row
        let (min_midi, max_midi) = node.notes.iter().fold((127_i32, 0_i32), |(mn, mx), ev| {
            (mn.min(ev.midi), mx.max(ev.midi))
        });
        let pitch_span = (max_midi - min_midi).max(1) as f32;
        let has_pitch_variation = max_midi > min_midi;

        // Draw every NoteEvent block on this track
        for (note_idx, ev) in node.notes.iter().enumerate() {
            let x0 = grid_left + ev.step * step_w + 1.0;
            let x1 = (grid_left + (ev.step + ev.duration.max(0.25)) * step_w - 1.0).max(x0 + 3.0);
            if x0 > grid_left + grid_width {
                continue;
            }

            let (ny0, ny1) = if has_pitch_variation {
                let norm_p = (ev.midi - min_midi) as f32 / pitch_span;
                let note_h = (row_h * 0.36).clamp(12.0, 22.0);
                let avail_h = (row_h - note_h - 8.0).max(4.0);
                let top = y1 - 4.0 - note_h - norm_p * avail_h;
                (top, top + note_h)
            } else {
                let vel_h = (row_h - 10.0) * (0.45 + 0.55 * ev.velocity.clamp(0.2, 1.0));
                (y1 - 5.0 - vel_h, y1 - 5.0)
            };

            let note_rect = Rect::from_min_max(Pos2::new(x0, ny0), Pos2::new(x1, ny1));
            let is_sounding =
                is_playing && is_active && playhead_step >= ev.step && playhead_step < ev.step + ev.duration;

            let note_id = (note_idx + 1) as u32;
            let is_entity_selected = scene.topology_selections.iter().any(|sel| {
                sel.node == node.key && sel.entity.identifier() == note_id
            });

            let alpha_mul = if is_active {
                (0.65 + 0.35 * ev.velocity).clamp(0.4, 1.0)
            } else {
                0.28
            };
            let fill = if is_sounding {
                Color32::from_rgb(
                    (r * 255.0 + 55.0).min(255.0) as u8,
                    (g * 255.0 + 55.0).min(255.0) as u8,
                    (b * 255.0 + 55.0).min(255.0) as u8,
                )
            } else {
                Color32::from_rgba_unmultiplied(
                    track_color.r(),
                    track_color.g(),
                    track_color.b(),
                    (alpha_mul * 235.0) as u8,
                )
            };

            painter.rect_filled(note_rect, 3.0, fill);
            let border_col = if is_entity_selected {
                Color32::WHITE
            } else if is_sounding {
                Color32::from_rgb(255, 250, 210)
            } else {
                Color32::from_rgba_unmultiplied(255, 255, 255, 65)
            };
            painter.rect_stroke(
                note_rect,
                3.0,
                Stroke::new(if is_entity_selected || is_sounding { 1.6 } else { 0.8 }, border_col),
                StrokeKind::Inside,
            );

            // Pitch slide / glide vector indicator (e.g. 808 or Acid 303 slide)
            if let Some(slide_target) = ev.slide_to {
                let target_norm = if has_pitch_variation {
                    ((slide_target - min_midi) as f32 / pitch_span).clamp(0.0, 1.0)
                } else if slide_target > ev.midi {
                    1.0
                } else {
                    0.0
                };
                let end_y = y1 - 6.0 - target_norm * (row_h - 12.0);
                painter.line_segment(
                    [
                        Pos2::new(note_rect.left() + 3.0, note_rect.center().y),
                        Pos2::new(note_rect.right() - 3.0, end_y),
                    ],
                    Stroke::new(2.0, Color32::from_rgb(255, 240, 150)),
                );
            }

            // Note pitch label if wide enough
            if note_rect.width() >= 22.0 && !ev.label.is_empty() {
                painter.text(
                    Pos2::new(note_rect.left() + 4.0, note_rect.center().y),
                    Align2::LEFT_CENTER,
                    &ev.label,
                    FontId::monospace(9.5),
                    Color32::from_rgb(15, 18, 24),
                );
            }

            // Click note block to select & inspect its musical parameters
            let note_resp = ui.allocate_rect(note_rect, Sense::click());
            if note_resp.clicked() {
                let extend = ui.input(|i| i.modifiers.command);
                events.push(ViewerEvent::Picked {
                    key: Some(node.key.clone()),
                    topology: Some(TopologySelection {
                        node: node.key.clone(),
                        part: 0,
                        entity: TopologyEntity::Face(note_id),
                        position: Vec3::new(ev.step, ev.midi as f32, ev.velocity),
                    }),
                    extend,
                });
            }
            if note_resp.hovered() {
                note_resp.on_hover_text(format!(
                    "{} — Step {:.2} (dur {:.2}) | Pitch {} (MIDI {}) | Vel {:.0}%",
                    node.name,
                    ev.step,
                    ev.duration,
                    ev.label,
                    ev.midi,
                    ev.velocity * 100.0
                ));
            }
        }

        // Row bottom divider
        painter.line_segment(
            [Pos2::new(rect.left(), y1), Pos2::new(rect.right(), y1)],
            Stroke::new(1.0, Color32::from_rgb(30, 34, 44)),
        );
    }

    // Header vertical border
    painter.line_segment(
        [
            Pos2::new(grid_left, rect.top()),
            Pos2::new(grid_left, rect.bottom()),
        ],
        Stroke::new(1.2, Color32::from_rgb(48, 52, 66)),
    );

    // Animated Playhead Cursor across the sequencer
    let playhead_x = grid_left + audio.playhead_fraction() * grid_width;
    painter.line_segment(
        [
            Pos2::new(playhead_x, rect.top()),
            Pos2::new(playhead_x, rect.bottom()),
        ],
        Stroke::new(2.0, Color32::from_rgb(255, 215, 64)),
    );
    painter.circle_filled(
        Pos2::new(playhead_x, ruler_rect.bottom() - 4.0),
        4.5,
        Color32::from_rgb(255, 215, 64),
    );
}

fn draw_waveform_and_scope(
    ui: &mut Ui,
    rect: Rect,
    scene: &Scene,
    audio: &AudioPlayer,
    full_page: bool,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(14, 16, 22));

    let title_h = 22.0;
    let header_rect = Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.top() + title_h));
    painter.rect_filled(header_rect, 0.0, Color32::from_rgb(22, 25, 33));
    painter.text(
        Pos2::new(rect.left() + 10.0, header_rect.center().y),
        Align2::LEFT_CENTER,
        "WAVEFORM ENVELOPE & LIVE OSCILLOSCOPE (CLICK TO SEEK)",
        FontId::proportional(11.0),
        Color32::from_rgb(145, 155, 180),
    );

    let content = Rect::from_min_max(Pos2::new(rect.left() + 8.0, header_rect.bottom() + 6.0), rect.max - Vec2::splat(8.0));
    if content.width() < 20.0 || content.height() < 20.0 {
        return;
    }

    // Allow clicking/dragging on the waveform to seek the playhead
    let seek_resp = ui.allocate_rect(content, Sense::click_and_drag());
    if (seek_resp.clicked() || seek_resp.dragged())
        && let Some(pos) = seek_resp.interact_pointer_pos()
    {
        let frac = ((pos.x - content.left()) / content.width()).clamp(0.0, 0.999);
        audio.seek_fraction(frac);
    }

    let selected_node = scene.current().or_else(|| scene.nodes.first());
    let mid_y = content.center().y;
    let half_h = content.height() * 0.45;

    // Zero-crossing axis line
    painter.line_segment(
        [Pos2::new(content.left(), mid_y), Pos2::new(content.right(), mid_y)],
        Stroke::new(1.0, Color32::from_rgb(36, 40, 52)),
    );

    // Draw waveform peaks (either per-track stacked in full_page mode, or master/selected in split mode)
    if full_page && scene.nodes.len() > 1 {
        let lane_h = content.height() / (scene.nodes.len() as f32);
        for (idx, node) in scene.nodes.iter().enumerate() {
            let lane_rect = Rect::from_min_max(
                Pos2::new(content.left(), content.top() + idx as f32 * lane_h),
                Pos2::new(content.right(), content.top() + (idx + 1) as f32 * lane_h),
            );
            draw_node_waveform(&painter, lane_rect, node, true);
        }
    } else if let Some(node) = selected_node {
        draw_node_waveform(&painter, content, node, false);
    }

    // Overlay live stereo oscilloscope trace when playing
    if audio.is_playing() {
        let scope = audio.scope_samples();
        if scope.len() >= 2 {
            let pts: Vec<Pos2> = scope
                .iter()
                .enumerate()
                .map(|(i, [l, r])| {
                    let x = content.left() + (i as f32 / (scope.len() - 1) as f32) * content.width();
                    let mono = (l + r) * 0.5;
                    let y = mid_y - mono.clamp(-1.0, 1.0) * half_h * 0.85;
                    Pos2::new(x, y)
                })
                .collect();
            painter.add(egui::Shape::line(
                pts,
                Stroke::new(1.5, Color32::from_rgba_unmultiplied(120, 255, 200, 190)),
            ));
        }
    }

    // Playhead line over waveform
    let playhead_x = content.left() + audio.playhead_fraction() * content.width();
    painter.line_segment(
        [
            Pos2::new(playhead_x, content.top()),
            Pos2::new(playhead_x, content.bottom()),
        ],
        Stroke::new(2.0, Color32::from_rgb(255, 215, 64)),
    );
}

fn draw_node_waveform(
    painter: &egui::Painter,
    rect: Rect,
    node: &SceneNode,
    show_label: bool,
) {
    let [r, g, b] = node
        .overrides
        .color
        .or(node.color)
        .unwrap_or([0.28, 0.78, 0.96]);
    let color = Color32::from_rgba_unmultiplied(
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8,
        200,
    );

    let mid_y = rect.center().y;
    let half_h = rect.height() * 0.44;
    let bins = node.waveform.len();
    if bins > 0 {
        let step_x = rect.width() / (bins as f32);
        for (i, &[mn, mx]) in node.waveform.iter().enumerate() {
            let x = rect.left() + (i as f32 + 0.5) * step_x;
            let y_top = mid_y - mx.clamp(-1.0, 1.0) * half_h;
            let y_bot = mid_y - mn.clamp(-1.0, 1.0) * half_h;
            painter.line_segment(
                [Pos2::new(x, y_top), Pos2::new(x, y_bot.max(y_top + 1.0))],
                Stroke::new((step_x * 0.75).max(1.2), color),
            );
        }
    }

    if show_label {
        painter.text(
            Pos2::new(rect.left() + 6.0, rect.top() + 10.0),
            Align2::LEFT_CENTER,
            &node.name,
            FontId::proportional(11.5),
            Color32::WHITE,
        );
    }
}

fn draw_spectrum_analyzer(
    _ui: &mut Ui,
    rect: Rect,
    scene: &Scene,
    audio: &AudioPlayer,
    _full_page: bool,
) {
    let painter = _ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(14, 16, 22));

    let title_h = 22.0;
    let header_rect = Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.top() + title_h));
    painter.rect_filled(header_rect, 0.0, Color32::from_rgb(22, 25, 33));
    painter.text(
        Pos2::new(rect.left() + 10.0, header_rect.center().y),
        Align2::LEFT_CENTER,
        "16-BAND FREQUENCY SPECTRUM ANALYZER (HZ)",
        FontId::proportional(11.0),
        Color32::from_rgb(145, 155, 180),
    );

    let content = Rect::from_min_max(
        Pos2::new(rect.left() + 12.0, header_rect.bottom() + 10.0),
        Pos2::new(rect.right() - 12.0, rect.bottom() - 22.0),
    );
    if content.width() < 40.0 || content.height() < 20.0 {
        return;
    }

    // Aggregate 16-band spectrum across active tracks (weighted by live track VU when playing)
    let mut bands = [0.0_f32; 16];
    let is_playing = audio.is_playing();
    let any_solo = scene.nodes.iter().any(SceneNode::is_solo);

    for node in &scene.nodes {
        let active = if any_solo {
            node.is_solo()
        } else {
            !node.is_muted()
        };
        if !active {
            continue;
        }
        let weight = if is_playing {
            (audio.track_vu(&node.key) * 1.4).clamp(0.08, 1.2)
        } else {
            node.effective_volume()
        };
        for (i, &val) in node.spectrum.iter().take(16).enumerate() {
            bands[i] = (bands[i] + val * weight).min(1.0);
        }
    }

    let n = 16;
    let slot_w = content.width() / (n as f32);
    let bar_w = (slot_w * 0.72).max(4.0);

    for (i, &val) in bands.iter().enumerate() {
        let x0 = content.left() + (i as f32) * slot_w + (slot_w - bar_w) * 0.5;
        let bar_h = content.height() * val.clamp(0.03, 1.0);
        let bar_rect = Rect::from_min_max(
            Pos2::new(x0, content.bottom() - bar_h),
            Pos2::new(x0 + bar_w, content.bottom()),
        );

        let t = i as f32 / 15.0;
        let col = Color32::from_rgb(
            (230.0 * (1.0 - t * 0.65) + 40.0) as u8,
            (95.0 + 145.0 * (1.0 - (t - 0.5).abs() * 1.6).max(0.0)) as u8,
            (110.0 + 145.0 * t) as u8,
        );
        painter.rect_filled(bar_rect, 2.0, col);

        if slot_w >= 18.0 || i % 2 == 0 {
            painter.text(
                Pos2::new(x0 + bar_w * 0.5, content.bottom() + 10.0),
                Align2::CENTER_CENTER,
                FREQ_BAND_LABELS[i],
                FontId::monospace(8.5),
                Color32::from_rgb(130, 140, 162),
            );
        }
    }
}
