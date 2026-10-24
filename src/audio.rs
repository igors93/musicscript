//! Real-time multi-track Beat Studio audio mixer and playback engine.
//!
//! Streams mixed 16-bit 44.1 kHz stereo PCM audio directly to the system audio
//! output (via `winmm` `waveOut` on Windows, with automatic software clock
//! fallback when no audio device is available) while computing live stereo VU
//! levels, per-track peak meters, and an oscilloscope waveform buffer.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::scene::Scene;

pub const DEFAULT_SAMPLE_RATE: u32 = 44_100;
const CHUNK_FRAMES: usize = 1024;
const NUM_BUFFERS: usize = 6;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct TrackMixSpec {
    pub key: String,
    pub samples: Arc<Vec<f32>>,
    pub sample_rate: u32,
    pub volume: f32,
    pub pan: f32,
    pub muted: bool,
    pub solo: bool,
}

struct SharedState {
    playing: AtomicBool,
    looping: AtomicBool,
    shutdown: AtomicBool,
    playhead_samples: AtomicU64,
    total_samples: AtomicU64,
    sample_rate: AtomicU32,
    master_volume_bits: AtomicU32,
    bpm_bits: AtomicU32,
    total_steps: AtomicU64,
    vu_left_bits: AtomicU32,
    vu_right_bits: AtomicU32,
    tracks: Mutex<Vec<TrackMixSpec>>,
    track_levels: Mutex<HashMap<String, f32>>,
    scope: Mutex<Vec<[f32; 2]>>,
}

impl SharedState {
    fn new() -> Self {
        Self {
            playing: AtomicBool::new(false),
            looping: AtomicBool::new(true),
            shutdown: AtomicBool::new(false),
            playhead_samples: AtomicU64::new(0),
            total_samples: AtomicU64::new((DEFAULT_SAMPLE_RATE * 4) as u64),
            sample_rate: AtomicU32::new(DEFAULT_SAMPLE_RATE),
            master_volume_bits: AtomicU32::new(0.9_f32.to_bits()),
            bpm_bits: AtomicU32::new(120.0_f32.to_bits()),
            total_steps: AtomicU64::new(32),
            vu_left_bits: AtomicU32::new(0.0_f32.to_bits()),
            vu_right_bits: AtomicU32::new(0.0_f32.to_bits()),
            tracks: Mutex::new(Vec::new()),
            track_levels: Mutex::new(HashMap::new()),
            scope: Mutex::new(vec![[0.0, 0.0]; 256]),
        }
    }

    fn mix_next_chunk(&self, out_pcm16: &mut [i16]) -> bool {
        let frames = out_pcm16.len() / 2;
        if !self.playing.load(Ordering::Relaxed) {
            out_pcm16.fill(0);
            self.decay_meters();
            return false;
        }

        let total = self.total_samples.load(Ordering::Relaxed).max(1);
        let looping = self.looping.load(Ordering::Relaxed);
        let master_vol = f32::from_bits(self.master_volume_bits.load(Ordering::Relaxed));

        let tracks_guard = self.tracks.lock().unwrap();
        if tracks_guard.is_empty() {
            out_pcm16.fill(0);
            self.decay_meters();
            return false;
        }

        let any_solo = tracks_guard.iter().any(|t| t.solo);
        let start_pos = self.playhead_samples.load(Ordering::Relaxed);

        let mut left_buf = vec![0.0_f32; frames];
        let mut right_buf = vec![0.0_f32; frames];
        let mut per_track_peaks: HashMap<String, f32> = HashMap::with_capacity(tracks_guard.len());

        for tr in tracks_guard.iter() {
            let active = if any_solo { tr.solo } else { !tr.muted };
            if !active || tr.samples.is_empty() || tr.volume <= 0.0001 {
                per_track_peaks.insert(tr.key.clone(), 0.0);
                continue;
            }

            let n_tr = tr.samples.len();
            let angle = (tr.pan.clamp(-1.0, 1.0) + 1.0) * (std::f32::consts::PI / 4.0);
            let l_gain = angle.cos() * tr.volume;
            let r_gain = angle.sin() * tr.volume;
            let mut tr_peak = 0.0_f32;

            for i in 0..frames {
                let abs_pos = start_pos + i as u64;
                if !looping && abs_pos >= total {
                    break;
                }
                let wrapped = (abs_pos % total) as usize;
                let sample = tr.samples[wrapped % n_tr];
                let abs_s = (sample * tr.volume).abs();
                if abs_s > tr_peak {
                    tr_peak = abs_s;
                }
                left_buf[i] += sample * l_gain;
                right_buf[i] += sample * r_gain;
            }
            per_track_peaks.insert(tr.key.clone(), tr_peak.min(1.2));
        }
        drop(tracks_guard);

        let mut peak_l = 0.0_f32;
        let mut peak_r = 0.0_f32;
        let mut scope_points = Vec::with_capacity(256);
        let scope_stride = (frames / 256).max(1);

        for i in 0..frames {
            let l = (left_buf[i] * master_vol).tanh();
            let r = (right_buf[i] * master_vol).tanh();
            if l.abs() > peak_l {
                peak_l = l.abs();
            }
            if r.abs() > peak_r {
                peak_r = r.abs();
            }
            if i % scope_stride == 0 && scope_points.len() < 256 {
                scope_points.push([l, r]);
            }
            out_pcm16[i * 2] = (l.clamp(-1.0, 1.0) * 32767.0) as i16;
            out_pcm16[i * 2 + 1] = (r.clamp(-1.0, 1.0) * 32767.0) as i16;
        }

        // Smooth VU meter ballistics
        let prev_l = f32::from_bits(self.vu_left_bits.load(Ordering::Relaxed));
        let prev_r = f32::from_bits(self.vu_right_bits.load(Ordering::Relaxed));
        let next_l = if peak_l > prev_l {
            peak_l
        } else {
            prev_l * 0.82 + peak_l * 0.18
        };
        let next_r = if peak_r > prev_r {
            peak_r
        } else {
            prev_r * 0.82 + peak_r * 0.18
        };
        self.vu_left_bits.store(next_l.to_bits(), Ordering::Relaxed);
        self.vu_right_bits.store(next_r.to_bits(), Ordering::Relaxed);

        if let Ok(mut tl) = self.track_levels.try_lock() {
            for (k, p) in per_track_peaks {
                let prev = tl.get(&k).copied().unwrap_or(0.0);
                let smoothed = if p > prev { p } else { prev * 0.80 + p * 0.20 };
                tl.insert(k, smoothed);
            }
        }
        if let Ok(mut sc) = self.scope.try_lock() {
            *sc = scope_points;
        }

        let next_pos = start_pos + frames as u64;
        if next_pos >= total {
            if looping {
                self.playhead_samples
                    .store(next_pos % total, Ordering::Relaxed);
            } else {
                self.playhead_samples.store(0, Ordering::Relaxed);
                self.playing.store(false, Ordering::Relaxed);
            }
        } else {
            self.playhead_samples.store(next_pos, Ordering::Relaxed);
        }
        true
    }

    fn decay_meters(&self) {
        let l = f32::from_bits(self.vu_left_bits.load(Ordering::Relaxed)) * 0.85;
        let r = f32::from_bits(self.vu_right_bits.load(Ordering::Relaxed)) * 0.85;
        self.vu_left_bits.store(
            (if l < 0.002 { 0.0 } else { l }).to_bits(),
            Ordering::Relaxed,
        );
        self.vu_right_bits.store(
            (if r < 0.002 { 0.0 } else { r }).to_bits(),
            Ordering::Relaxed,
        );
        if let Ok(mut tl) = self.track_levels.try_lock() {
            for val in tl.values_mut() {
                *val *= 0.85;
                if *val < 0.002 {
                    *val = 0.0;
                }
            }
        }
    }
}

pub struct AudioPlayer {
    state: Arc<SharedState>,
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioPlayer {
    pub fn new() -> Self {
        let state = Arc::new(SharedState::new());
        let thread_state = Arc::clone(&state);
        thread::Builder::new()
            .name("musicscript-audio".into())
            .spawn(move || {
                audio_thread_main(thread_state);
            })
            .ok();
        Self { state }
    }

    /// Synchronize active tracks, live mixer overrides, BPM, and loop length from `Scene`.
    pub fn sync_from_scene(&self, scene: &Scene) {
        let mut specs = Vec::with_capacity(scene.nodes.len());
        let mut max_samples: u64 = 0;
        let mut max_steps: usize = 32;
        let mut bpm: f32 = 120.0;
        let mut sr: u32 = DEFAULT_SAMPLE_RATE;

        for node in &scene.nodes {
            if node.bpm > 0.0 {
                bpm = node.bpm;
            }
            if node.steps > max_steps {
                max_steps = node.steps;
            }
            if node.sample_rate > 0 {
                sr = node.sample_rate;
            }
            let len = node.audio_pcm.len() as u64;
            if len > max_samples {
                max_samples = len;
            }
            specs.push(TrackMixSpec {
                key: node.key.clone(),
                samples: Arc::clone(&node.audio_pcm),
                sample_rate: node.sample_rate.max(8000),
                volume: node.effective_volume(),
                pan: node.effective_pan(),
                muted: node.is_muted(),
                solo: node.is_solo(),
            });
        }

        if max_samples == 0 {
            let step_sec = (60.0 / bpm.max(30.0)) / 4.0;
            max_samples = ((max_steps as f32 * step_sec) * sr as f32).round() as u64;
        }

        self.state.sample_rate.store(sr, Ordering::Relaxed);
        self.state
            .total_samples
            .store(max_samples.max(1), Ordering::Relaxed);
        self.state
            .bpm_bits
            .store(bpm.to_bits(), Ordering::Relaxed);
        self.state
            .total_steps
            .store(max_steps.max(1) as u64, Ordering::Relaxed);

        let cur_pos = self.state.playhead_samples.load(Ordering::Relaxed);
        if cur_pos >= max_samples && max_samples > 0 {
            self.state
                .playhead_samples
                .store(cur_pos % max_samples, Ordering::Relaxed);
        }

        if let Ok(mut guard) = self.state.tracks.lock() {
            *guard = specs;
        }
    }

    pub fn play(&self) {
        self.state.playing.store(true, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn pause(&self) {
        self.state.playing.store(false, Ordering::Relaxed);
    }

    pub fn toggle_play(&self) {
        let was_playing = self.state.playing.load(Ordering::Relaxed);
        self.state.playing.store(!was_playing, Ordering::Relaxed);
    }

    pub fn stop(&self) {
        self.state.playing.store(false, Ordering::Relaxed);
        self.state.playhead_samples.store(0, Ordering::Relaxed);
        self.state.decay_meters();
    }

    pub fn is_playing(&self) -> bool {
        self.state.playing.load(Ordering::Relaxed)
    }

    pub fn is_looping(&self) -> bool {
        self.state.looping.load(Ordering::Relaxed)
    }

    pub fn set_looping(&self, looping: bool) {
        self.state.looping.store(looping, Ordering::Relaxed);
    }

    pub fn master_volume(&self) -> f32 {
        f32::from_bits(self.state.master_volume_bits.load(Ordering::Relaxed))
    }

    pub fn set_master_volume(&self, volume: f32) {
        self.state
            .master_volume_bits
            .store(volume.clamp(0.0, 1.5).to_bits(), Ordering::Relaxed);
    }

    pub fn seek_fraction(&self, fraction: f32) {
        let total = self.state.total_samples.load(Ordering::Relaxed).max(1);
        let target = ((fraction.clamp(0.0, 0.9999) as f64) * (total as f64)) as u64;
        self.state.playhead_samples.store(target, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn seek_seconds(&self, seconds: f32) {
        let sr = self.state.sample_rate.load(Ordering::Relaxed).max(1) as f64;
        let total = self.state.total_samples.load(Ordering::Relaxed).max(1);
        let target = ((seconds.max(0.0) as f64) * sr) as u64;
        self.state
            .playhead_samples
            .store(target.min(total.saturating_sub(1)), Ordering::Relaxed);
    }

    pub fn playhead_seconds(&self) -> f32 {
        let sr = self.state.sample_rate.load(Ordering::Relaxed).max(1) as f32;
        let pos = self.state.playhead_samples.load(Ordering::Relaxed) as f32;
        pos / sr
    }

    pub fn duration_seconds(&self) -> f32 {
        let sr = self.state.sample_rate.load(Ordering::Relaxed).max(1) as f32;
        let total = self.state.total_samples.load(Ordering::Relaxed).max(1) as f32;
        total / sr
    }

    pub fn playhead_fraction(&self) -> f32 {
        let total = self.state.total_samples.load(Ordering::Relaxed).max(1) as f32;
        let pos = self.state.playhead_samples.load(Ordering::Relaxed) as f32;
        (pos / total).clamp(0.0, 1.0)
    }

    pub fn bpm(&self) -> f32 {
        f32::from_bits(self.state.bpm_bits.load(Ordering::Relaxed))
    }

    pub fn total_steps(&self) -> usize {
        self.state.total_steps.load(Ordering::Relaxed).max(1) as usize
    }

    pub fn current_step(&self) -> f32 {
        self.playhead_fraction() * (self.total_steps() as f32)
    }

    pub fn stereo_vu(&self) -> [f32; 2] {
        [
            f32::from_bits(self.state.vu_left_bits.load(Ordering::Relaxed)),
            f32::from_bits(self.state.vu_right_bits.load(Ordering::Relaxed)),
        ]
    }

    pub fn track_vu(&self, key: &str) -> f32 {
        self.state
            .track_levels
            .lock()
            .ok()
            .and_then(|m| m.get(key).copied())
            .unwrap_or(0.0)
    }

    pub fn scope_samples(&self) -> Vec<[f32; 2]> {
        self.state
            .scope
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.state.shutdown.store(true, Ordering::Relaxed);
    }
}

#[cfg(target_os = "windows")]
mod winmm_out {
    use super::*;
    use std::ffi::c_void;
    use std::ptr;

    const WAVE_FORMAT_PCM: u16 = 1;
    const WAVE_MAPPER: u32 = u32::MAX;
    const WHDR_DONE: u32 = 0x0000_0001;
    const MMSYSERR_NOERROR: u32 = 0;

    #[repr(C)]
    struct WaveFormatEx {
        w_format_tag: u16,
        n_channels: u16,
        n_samples_per_sec: u32,
        n_avg_bytes_per_sec: u32,
        n_block_align: u16,
        w_bits_per_sample: u16,
        cb_size: u16,
    }

    #[repr(C)]
    struct WaveHdr {
        lp_data: *mut i8,
        dw_buffer_length: u32,
        dw_bytes_recorded: u32,
        dw_user: usize,
        dw_flags: u32,
        dw_loops: u32,
        lp_next: *mut WaveHdr,
        reserved: usize,
    }

    #[link(name = "winmm")]
    unsafe extern "system" {
        fn waveOutOpen(
            phwo: *mut *mut c_void,
            u_device_id: u32,
            pwfx: *const WaveFormatEx,
            dw_callback: usize,
            dw_instance: usize,
            fdw_open: u32,
        ) -> u32;
        fn waveOutPrepareHeader(hwo: *mut c_void, pwh: *mut WaveHdr, cbwh: u32) -> u32;
        fn waveOutUnprepareHeader(hwo: *mut c_void, pwh: *mut WaveHdr, cbwh: u32) -> u32;
        fn waveOutWrite(hwo: *mut c_void, pwh: *mut WaveHdr, cbwh: u32) -> u32;
        fn waveOutReset(hwo: *mut c_void) -> u32;
        fn waveOutClose(hwo: *mut c_void) -> u32;
    }

    pub fn run_winmm_loop(state: &Arc<SharedState>) -> bool {
        let wfx = WaveFormatEx {
            w_format_tag: WAVE_FORMAT_PCM,
            n_channels: 2,
            n_samples_per_sec: DEFAULT_SAMPLE_RATE,
            n_avg_bytes_per_sec: DEFAULT_SAMPLE_RATE * 4,
            n_block_align: 4,
            w_bits_per_sample: 16,
            cb_size: 0,
        };

        let mut hwo: *mut c_void = ptr::null_mut();
        let res = unsafe { waveOutOpen(&mut hwo, WAVE_MAPPER, &wfx, 0, 0, 0) };
        if res != MMSYSERR_NOERROR || hwo.is_null() {
            return false;
        }

        let samples_per_buf = CHUNK_FRAMES * 2;
        let bytes_per_buf = (samples_per_buf * std::mem::size_of::<i16>()) as u32;
        let mut buffers: Vec<Box<[i16]>> = (0..NUM_BUFFERS)
            .map(|_| vec![0_i16; samples_per_buf].into_boxed_slice())
            .collect();
        let mut headers: Vec<WaveHdr> = buffers
            .iter_mut()
            .map(|buf| WaveHdr {
                lp_data: buf.as_mut_ptr() as *mut i8,
                dw_buffer_length: bytes_per_buf,
                dw_bytes_recorded: 0,
                dw_user: 0,
                dw_flags: WHDR_DONE,
                dw_loops: 0,
                lp_next: ptr::null_mut(),
                reserved: 0,
            })
            .collect();

        for hdr in &mut headers {
            unsafe {
                waveOutPrepareHeader(hwo, hdr, std::mem::size_of::<WaveHdr>() as u32);
                hdr.dw_flags |= WHDR_DONE;
            }
        }

        while !state.shutdown.load(Ordering::Relaxed) {
            if !state.playing.load(Ordering::Relaxed) {
                state.decay_meters();
                thread::sleep(Duration::from_millis(12));
                continue;
            }

            let mut queued_any = false;
            for idx in 0..NUM_BUFFERS {
                if (headers[idx].dw_flags & WHDR_DONE) != 0 {
                    let active = state.mix_next_chunk(&mut buffers[idx]);
                    if active {
                        headers[idx].dw_flags &= !WHDR_DONE;
                        unsafe {
                            waveOutWrite(
                                hwo,
                                &mut headers[idx],
                                std::mem::size_of::<WaveHdr>() as u32,
                            );
                        }
                        queued_any = true;
                    }
                }
            }

            if !queued_any {
                thread::sleep(Duration::from_millis(4));
            }
        }

        unsafe {
            waveOutReset(hwo);
            for hdr in &mut headers {
                waveOutUnprepareHeader(hwo, hdr, std::mem::size_of::<WaveHdr>() as u32);
            }
            waveOutClose(hwo);
        }
        true
    }
}

fn audio_thread_main(state: Arc<SharedState>) {
    #[cfg(target_os = "windows")]
    {
        if winmm_out::run_winmm_loop(&state) {
            return;
        }
    }

    // Fallback software mixer loop if no hardware output device is present
    let mut scratch = vec![0_i16; CHUNK_FRAMES * 2];
    let chunk_dur = Duration::from_secs_f64(CHUNK_FRAMES as f64 / DEFAULT_SAMPLE_RATE as f64);
    let mut next_tick = Instant::now();
    while !state.shutdown.load(Ordering::Relaxed) {
        state.mix_next_chunk(&mut scratch);
        next_tick += chunk_dur;
        let now = Instant::now();
        if next_tick > now {
            thread::sleep(next_tick - now);
        } else {
            next_tick = now;
        }
    }
}
