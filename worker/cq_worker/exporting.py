"""Export displayed MusicScript Tracks to WAV, MIDI, and individual Stems."""

from __future__ import annotations

import os

from .scene import Registry
from .synth import SAMPLE_RATE, mix_tracks_stereo, write_midi_file, write_wav_file

FORMATS = ("wav", "mid", "midi", "stems", "step", "stl", "brep")


def export(
    registry: Registry,
    keys: list[str],
    file_format: str,
    path: str,
    tolerance: float = 0.1,
    angular_tolerance: float = 0.1,
) -> None:
    """Export the Tracks behind ``keys`` (or all tracks if ``keys`` is empty) to WAV, MIDI, or Stems."""
    del tolerance, angular_tolerance
    fmt = file_format.lower().strip()
    if fmt not in FORMATS:
        raise ValueError(f"Unsupported export format {file_format!r}")

    target_keys = keys if keys else registry.keys()
    tracks = registry.subtree_shapes(target_keys)
    if not tracks:
        raise ValueError("No tracks to export; run a beat script first.")

    out_dir = os.path.dirname(os.path.abspath(path))
    if out_dir and not os.path.exists(out_dir):
        os.makedirs(out_dir, exist_ok=True)

    # MIDI export ("mid", "midi", or legacy "step" enum alias)
    if fmt in ("mid", "midi", "step"):
        midi_path = path if path.lower().endswith((".mid", ".midi")) else f"{os.path.splitext(path)[0]}.mid"
        write_midi_file(midi_path, tracks)
        return

    # Multi-track Stems export ("stems" or legacy "brep" enum alias)
    if fmt in ("stems", "brep"):
        base_root, _ = os.path.splitext(path)
        for idx, tr in enumerate(tracks):
            safe_name = "".join(c if c.isalnum() or c in ("_", "-") else "_" for c in tr.name)
            stem_path = f"{base_root}_{idx + 1:02d}_{safe_name}.wav"
            stereo_stem = mix_tracks_stereo([tr], sample_rate=SAMPLE_RATE, master_volume=0.95)
            write_wav_file(stem_path, stereo_stem, sample_rate=SAMPLE_RATE)
        master_mix = mix_tracks_stereo(tracks, sample_rate=SAMPLE_RATE, master_volume=0.92)
        write_wav_file(f"{base_root}_master.wav", master_mix, sample_rate=SAMPLE_RATE)
        return

    # Master WAV mix ("wav" or legacy "stl" enum alias)
    wav_path = path if path.lower().endswith(".wav") else f"{os.path.splitext(path)[0]}.wav"
    stereo = mix_tracks_stereo(tracks, sample_rate=SAMPLE_RATE, master_volume=0.92)
    write_wav_file(wav_path, stereo, sample_rate=SAMPLE_RATE)
