"""MusicScript — Native Python Beat & Synthesizer Engine powered by NumPy.

Provides a fluent API for creating beats, drum patterns, basslines, chord
progressions, arpeggios, synthesizers, and DSP effects chains from code.
"""

from __future__ import annotations

import base64
from dataclasses import dataclass, field
import io
import math
import struct
import wave
from typing import Any

import numpy as np

SAMPLE_RATE = 44100

# Default track color palette (RGB 0..1)
TRACK_PALETTE: list[tuple[float, float, float]] = [
    (0.95, 0.35, 0.40),  # Coral Red (Kick)
    (0.98, 0.65, 0.22),  # Amber Gold (Snare / Clap)
    (0.95, 0.88, 0.28),  # Bright Yellow (Hi-Hat)
    (0.68, 0.38, 0.96),  # Deep Purple (808 / Sub Bass)
    (0.24, 0.76, 0.96),  # Cyan Blue (Synth Lead / Pluck)
    (0.28, 0.88, 0.56),  # Mint Green (Pad / Chords)
    (0.96, 0.42, 0.78),  # Neon Pink (Arp / Bell)
    (0.45, 0.58, 0.98),  # Indigo (Acid / FX)
]

INSTRUMENT_DEFAULT_COLORS: dict[str, tuple[float, float, float]] = {
    "kick": (0.95, 0.34, 0.38),
    "snare": (0.98, 0.64, 0.22),
    "clap": (0.98, 0.52, 0.28),
    "hihat": (0.95, 0.88, 0.28),
    "closed_hat": (0.95, 0.88, 0.28),
    "open_hat": (0.92, 0.78, 0.20),
    "perc": (0.86, 0.55, 0.36),
    "rim": (0.86, 0.55, 0.36),
    "tom": (0.88, 0.44, 0.32),
    "cowbell": (0.90, 0.80, 0.35),
    "bass808": (0.68, 0.36, 0.96),
    "808": (0.68, 0.36, 0.96),
    "sub": (0.56, 0.34, 0.92),
    "acid": (0.52, 0.95, 0.28),
    "saw": (0.24, 0.76, 0.96),
    "supersaw": (0.20, 0.82, 0.98),
    "synth": (0.24, 0.76, 0.96),
    "square": (0.35, 0.66, 0.98),
    "pulse": (0.35, 0.66, 0.98),
    "sine": (0.42, 0.84, 0.88),
    "triangle": (0.40, 0.86, 0.72),
    "pluck": (0.28, 0.90, 0.64),
    "pad": (0.32, 0.84, 0.54),
    "epiano": (0.48, 0.74, 0.96),
    "keys": (0.48, 0.74, 0.96),
    "fm": (0.88, 0.42, 0.86),
    "bell": (0.96, 0.42, 0.78),
    "arp": (0.96, 0.48, 0.72),
}

INSTRUMENT_DEFAULT_MIDI: dict[str, int] = {
    "kick": 36,       # C1
    "snare": 38,      # D1
    "clap": 39,       # D#1
    "rim": 37,        # C#1
    "perc": 37,       # C#1
    "tom": 45,        # A1
    "hihat": 42,      # F#1
    "closed_hat": 42, # F#1
    "open_hat": 46,   # A#1
    "cowbell": 56,    # G#2
    "bass808": 36,    # C2 (MIDI 36)
    "808": 36,
    "sub": 36,
    "acid": 48,       # C3
    "saw": 60,        # C4
    "supersaw": 60,
    "synth": 60,
    "square": 60,
    "pulse": 60,
    "sine": 60,
    "triangle": 60,
    "pluck": 60,
    "pad": 60,
    "epiano": 60,
    "keys": 60,
    "fm": 60,
    "bell": 72,       # C5
    "arp": 60,
}

NOTE_OFFSETS: dict[str, int] = {
    "C": 0,
    "C#": 1, "DB": 1,
    "D": 2,
    "D#": 3, "EB": 3,
    "E": 4, "FB": 4,
    "F": 5, "E#": 5,
    "F#": 6, "GB": 6,
    "G": 7,
    "G#": 8, "AB": 8,
    "A": 9,
    "A#": 10, "BB": 10,
    "B": 11, "CB": 11,
}

NOTE_NAMES_SHARP = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]

SCALES: dict[str, list[int]] = {
    "major": [0, 2, 4, 5, 7, 9, 11],
    "minor": [0, 2, 3, 5, 7, 8, 10],
    "natural_minor": [0, 2, 3, 5, 7, 8, 10],
    "harmonic_minor": [0, 2, 3, 5, 7, 8, 11],
    "melodic_minor": [0, 2, 3, 5, 7, 9, 11],
    "dorian": [0, 2, 3, 5, 7, 9, 10],
    "phrygian": [0, 1, 3, 5, 7, 8, 10],
    "lydian": [0, 2, 4, 6, 7, 9, 11],
    "mixolydian": [0, 2, 4, 5, 7, 9, 10],
    "pentatonic_minor": [0, 3, 5, 7, 10],
    "pentatonic_major": [0, 2, 4, 7, 9],
    "blues": [0, 3, 5, 6, 7, 10],
    "trap": [0, 1, 3, 5, 7, 8, 10],       # Phrygian dark trap scale
    "hirajoshi": [0, 2, 3, 7, 8],         # Japanese dark plucks
}

CHORD_INTERVALS: dict[str, list[int]] = {
    "": [0, 4, 7],
    "maj": [0, 4, 7],
    "M": [0, 4, 7],
    "m": [0, 3, 7],
    "min": [0, 3, 7],
    "-": [0, 3, 7],
    "dim": [0, 3, 6],
    "aug": [0, 4, 8],
    "sus2": [0, 2, 7],
    "sus4": [0, 5, 7],
    "5": [0, 7],
    "7": [0, 4, 7, 10],
    "maj7": [0, 4, 7, 11],
    "M7": [0, 4, 7, 11],
    "m7": [0, 3, 7, 10],
    "min7": [0, 3, 7, 10],
    "m9": [0, 3, 7, 10, 14],
    "maj9": [0, 4, 7, 11, 14],
    "9": [0, 4, 7, 10, 14],
    "add9": [0, 4, 7, 14],
    "madd9": [0, 3, 7, 14],
    "dim7": [0, 3, 6, 9],
    "m7b5": [0, 3, 6, 10],
}

# Global session state reset before each script execution
_GLOBAL_BPM: float = 120.0
_GLOBAL_SWING: float = 0.0
_GLOBAL_BARS: int = 2
_GLOBAL_STEPS_PER_BAR: int = 16
_REGISTERED_TRACKS: list["Track"] = []


def reset_session() -> None:
    """Reset global musical defaults before running a user script."""
    global _GLOBAL_BPM, _GLOBAL_SWING, _GLOBAL_BARS, _GLOBAL_STEPS_PER_BAR
    _GLOBAL_BPM = 120.0
    _GLOBAL_SWING = 0.0
    _GLOBAL_BARS = 2
    _GLOBAL_STEPS_PER_BAR = 16
    _REGISTERED_TRACKS.clear()


def set_bpm(bpm: float) -> float:
    """Set the default global BPM for tracks created in the script."""
    global _GLOBAL_BPM
    _GLOBAL_BPM = float(max(30.0, min(300.0, bpm)))
    return _GLOBAL_BPM


def get_bpm() -> float:
    """Return the current global BPM."""
    return _GLOBAL_BPM


def set_swing(swing: float) -> float:
    """Set the default global swing amount (0.0 to 0.75)."""
    global _GLOBAL_SWING
    _GLOBAL_SWING = float(max(0.0, min(0.75, swing)))
    return _GLOBAL_SWING


def set_bars(bars: int, steps_per_bar: int = 16) -> int:
    """Set the default bar count and steps per bar."""
    global _GLOBAL_BARS, _GLOBAL_STEPS_PER_BAR
    _GLOBAL_BARS = max(1, int(bars))
    _GLOBAL_STEPS_PER_BAR = max(4, int(steps_per_bar))
    return _GLOBAL_BARS


def note_to_midi(note: int | float | str, default_octave: int = 4) -> int:
    """Convert a note name (e.g. 'C4', 'F#2', 'Eb3') or MIDI number to an integer MIDI pitch."""
    if isinstance(note, (int, float)):
        val = int(round(note))
        return max(12, min(120, val))
    s = str(note).strip()
    if not s:
        return 12 * (default_octave + 1)
    if s.lstrip("-").isdigit():
        return max(12, min(120, int(s)))

    # Extract letter + accidental + optional octave
    head = s[0].upper()
    rest = s[1:]
    accidental = ""
    if rest and rest[0] in ("#", "b", "B"):
        accidental = "#" if rest[0] == "#" else "B"
        rest = rest[1:]
    key = head + accidental
    semitone = NOTE_OFFSETS.get(key, 0)
    try:
        octave = int(rest) if rest else default_octave
    except ValueError:
        octave = default_octave
    return max(12, min(120, (octave + 1) * 12 + semitone))


def midi_to_note(midi: int) -> str:
    """Format a MIDI pitch integer into standard scientific pitch notation (e.g. 60 -> 'C4')."""
    m = max(0, min(127, int(round(midi))))
    octave = (m // 12) - 1
    name = NOTE_NAMES_SHARP[m % 12]
    return f"{name}{octave}"


def midi_to_freq(midi: float) -> float:
    """Convert a MIDI pitch to frequency in Hz (A4 = 69 = 440 Hz)."""
    return 440.0 * (2.0 ** ((float(midi) - 69.0) / 12.0))


def note_to_freq(note: int | float | str, default_octave: int = 4) -> float:
    """Convert a note string or MIDI number directly to frequency in Hz."""
    return midi_to_freq(note_to_midi(note, default_octave=default_octave))


def chord_notes(chord_spec: str | list[str | int], default_octave: int = 4) -> list[int]:
    """Parse a chord symbol (e.g. 'Cm7', 'F#maj7', 'Ab', 'G7') or list of notes into MIDI pitches."""
    if isinstance(chord_spec, (list, tuple)):
        return [note_to_midi(n, default_octave=default_octave) for n in chord_spec]
    s = str(chord_spec).strip()
    if not s:
        return [60, 63, 67]

    # Check if it's a single note with octave like "C4"
    head = s[0].upper()
    rest = s[1:]
    accidental = ""
    if rest and rest[0] in ("#", "b"):
        accidental = "#" if rest[0] == "#" else "B"
        rest = rest[1:]

    octave = default_octave
    # Check trailing octave digit if preceded by chord quality or standalone
    if rest and rest[-1].isdigit() and rest not in CHORD_INTERVALS:
        # e.g. Cm7_4 or C4
        if len(rest) == 1:
            return [note_to_midi(s, default_octave=default_octave)]
        if "_" in rest:
            qual, oct_s = rest.rsplit("_", 1)
            if oct_s.isdigit():
                octave = int(oct_s)
                rest = qual

    root_midi = (octave + 1) * 12 + NOTE_OFFSETS.get(head + accidental, 0)
    intervals = CHORD_INTERVALS.get(rest, CHORD_INTERVALS.get(rest.lower(), [0, 3, 7]))
    return [root_midi + iv for iv in intervals]


def scale_notes(
    root: str = "C",
    scale: str = "minor",
    octave: int = 4,
    count: int = 8,
) -> list[str]:
    """Return a list of note strings in the given root and scale."""
    root_midi = note_to_midi(f"{root}{octave}", default_octave=octave)
    intervals = SCALES.get(scale.lower(), SCALES["minor"])
    result: list[str] = []
    for i in range(count):
        oct_shift = i // len(intervals)
        iv = intervals[i % len(intervals)]
        result.append(midi_to_note(root_midi + oct_shift * 12 + iv))
    return result


@dataclass
class NoteEvent:
    """A single scheduled note or drum trigger on a Track."""

    step: float
    duration: float
    midi: int
    velocity: float = 0.85
    label: str = ""
    slide_to: int | None = None

    def to_dict(self) -> dict[str, Any]:
        d: dict[str, Any] = {
            "step": round(float(self.step), 4),
            "duration": round(float(self.duration), 4),
            "midi": int(self.midi),
            "velocity": round(float(self.velocity), 3),
            "label": self.label or midi_to_note(self.midi),
        }
        if self.slide_to is not None:
            d["slide_to"] = int(self.slide_to)
        return d


@dataclass
class EffectSpec:
    """A DSP effect stage attached to a Track's signal chain."""

    kind: str
    params: dict[str, Any] = field(default_factory=dict)
    line: int | None = None


class _FluentFloat(float):
    """A float value on a Track that can also be called as a fluent setter method, e.g. `track.pan(-0.2)`."""

    _owner: Any = None
    _attr: str = ""

    def __new__(cls, value: float, owner: Any = None, attr: str = "") -> "_FluentFloat":
        obj = super().__new__(cls, float(value))
        obj._owner = owner
        obj._attr = attr
        return obj

    def __call__(self, new_value: float) -> Any:
        if self._owner is not None and self._attr:
            if self._attr == "pan":
                return self._owner.set_pan(new_value)
            if self._attr == "volume":
                return self._owner.gain(new_value)
        return self._owner


class Track:
    """A single musical track (drum, bass, synth, chord pad, or arpeggio) in MusicScript."""

    def __init__(
        self,
        name: str = "Track",
        instrument: str = "kick",
        pattern: str | list[Any] | None = None,
        notes: str | list[Any] | None = None,
        chords: list[Any] | str | None = None,
        steps_per_chord: float = 8.0,
        bpm: float | None = None,
        bars: int | None = None,
        steps: int | None = None,
        steps_per_bar: int | None = None,
        swing: float | None = None,
        volume: float = 0.85,
        pan: float = 0.0,
        octave: int | None = None,
        pitch: int | str | None = None,
        attack: float | None = None,
        decay: float | None = None,
        sustain: float | None = None,
        release: float | None = None,
        cutoff: float | None = None,
        resonance: float = 0.25,
        color: tuple[float, float, float] | list[float] | str | None = None,
        muted: bool = False,
        solo: bool = False,
    ) -> None:
        self.name = str(name)
        self.instrument = str(instrument).lower().strip()
        self.bpm = float(bpm if bpm is not None else _GLOBAL_BPM)
        self.steps_per_bar = int(steps_per_bar if steps_per_bar is not None else _GLOBAL_STEPS_PER_BAR)
        if steps is not None:
            self.total_steps = max(1, int(steps))
            self.bars = max(1, math.ceil(self.total_steps / self.steps_per_bar))
        else:
            self.bars = max(1, int(bars if bars is not None else _GLOBAL_BARS))
            self.total_steps = self.bars * self.steps_per_bar

        self.swing = float(swing if swing is not None else _GLOBAL_SWING)
        self.volume = _FluentFloat(max(0.0, min(1.5, volume)), self, "volume")
        self.pan = _FluentFloat(max(-1.0, min(1.0, pan)), self, "pan")
        self.muted = bool(muted)
        self.solo = bool(solo)

        default_midi = INSTRUMENT_DEFAULT_MIDI.get(self.instrument, 60)
        if pitch is not None:
            self.base_midi = note_to_midi(pitch)
        elif octave is not None:
            self.base_midi = (int(octave) + 1) * 12 + (default_midi % 12)
        else:
            self.base_midi = default_midi

        self.default_octave = (self.base_midi // 12) - 1

        # Envelope defaults tailored to instrument type
        is_drum = self.instrument in (
            "kick", "snare", "clap", "hihat", "closed_hat", "open_hat", "perc", "rim", "tom", "cowbell"
        )
        if is_drum:
            self.attack = float(attack if attack is not None else 0.002)
            self.decay = float(decay if decay is not None else self._default_drum_decay())
            self.sustain = float(sustain if sustain is not None else 0.0)
            self.release = float(release if release is not None else 0.04)
        elif self.instrument in ("bass808", "808", "sub"):
            self.attack = float(attack if attack is not None else 0.006)
            self.decay = float(decay if decay is not None else 0.45)
            self.sustain = float(sustain if sustain is not None else 0.35)
            self.release = float(release if release is not None else 0.12)
        elif self.instrument in ("pluck", "bell", "arp"):
            self.attack = float(attack if attack is not None else 0.004)
            self.decay = float(decay if decay is not None else 0.22)
            self.sustain = float(sustain if sustain is not None else 0.15)
            self.release = float(release if release is not None else 0.15)
        elif self.instrument in ("pad",):
            self.attack = float(attack if attack is not None else 0.12)
            self.decay = float(decay if decay is not None else 0.35)
            self.sustain = float(sustain if sustain is not None else 0.75)
            self.release = float(release if release is not None else 0.35)
        else:
            self.attack = float(attack if attack is not None else 0.01)
            self.decay = float(decay if decay is not None else 0.18)
            self.sustain = float(sustain if sustain is not None else 0.55)
            self.release = float(release if release is not None else 0.12)

        self.color = self._resolve_color(color)
        self.events: list[NoteEvent] = []
        self.effects: list[EffectSpec] = []
        self._pattern_str: str = ""
        self._notes_str: str = ""
        self._rendered_audio: np.ndarray | None = None

        if cutoff is not None:
            self.filter(cutoff=cutoff, resonance=resonance, kind="lowpass")

        if pattern is not None:
            self.pattern(pattern)
        if notes is not None:
            self.notes(notes)
        if chords is not None:
            self.chords(chords, steps_per_chord=steps_per_chord, octave=octave)

        if self not in _REGISTERED_TRACKS:
            _REGISTERED_TRACKS.append(self)

    def _default_drum_decay(self) -> float:
        return {
            "kick": 0.28,
            "snare": 0.19,
            "clap": 0.22,
            "hihat": 0.055,
            "closed_hat": 0.055,
            "open_hat": 0.28,
            "perc": 0.09,
            "rim": 0.07,
            "tom": 0.24,
            "cowbell": 0.16,
        }.get(self.instrument, 0.18)

    def _resolve_color(
        self, color: tuple[float, float, float] | list[float] | str | None
    ) -> tuple[float, float, float]:
        if isinstance(color, (tuple, list)) and len(color) >= 3:
            r, g, b = float(color[0]), float(color[1]), float(color[2])
            if r > 1.0 or g > 1.0 or b > 1.0:
                return (r / 255.0, g / 255.0, b / 255.0)
            return (r, g, b)
        if isinstance(color, str) and color.startswith("#") and len(color) in (7, 9):
            r = int(color[1:3], 16) / 255.0
            g = int(color[3:5], 16) / 255.0
            b = int(color[5:7], 16) / 255.0
            return (r, g, b)
        if self.instrument in INSTRUMENT_DEFAULT_COLORS:
            return INSTRUMENT_DEFAULT_COLORS[self.instrument]
        idx = len(_REGISTERED_TRACKS) % len(TRACK_PALETTE)
        return TRACK_PALETTE[idx]

    def _invalidate(self) -> None:
        self._rendered_audio = None

    @property
    def step_duration(self) -> float:
        """Duration of one 16th-note step in seconds."""
        return (60.0 / max(30.0, self.bpm)) / 4.0

    @property
    def duration(self) -> float:
        """Total loop duration of this track in seconds."""
        return self.total_steps * self.step_duration

    # ── Fluent Pattern / Note Builders ──────────────────────────────────

    def pattern(
        self,
        pat: str | list[Any],
        pitch: int | str | None = None,
        velocity: float = 0.9,
        repeat: bool = True,
    ) -> "Track":
        """Schedule hits from a step string like 'x...x...x...x...' or a list of steps.

        Characters:
        - 'x' or '1': normal hit
        - 'X' or '!': accented hit (1.15x velocity)
        - 'o' or 'g': ghost hit (0.48x velocity)
        - 'r' or '*': double 32nd-note roll on that step
        - 't': triple ratchet roll on that step
        - '-' or '_': sustain previous hit
        - '.' or '0' or ' ': rest
        """
        self._invalidate()
        midi = note_to_midi(pitch, default_octave=self.default_octave) if pitch is not None else self.base_midi

        if isinstance(pat, (list, tuple)):
            tokens = list(pat)
            self._pattern_str = "".join("x" if t else "." for t in tokens)
        else:
            raw = str(pat).replace("|", "").replace(" ", "")
            self._pattern_str = raw
            tokens = list(raw)

        if not tokens:
            return self

        n_tokens = len(tokens)
        target_steps = self.total_steps if repeat else n_tokens
        if not repeat and n_tokens > self.total_steps:
            self.total_steps = n_tokens
            self.bars = max(1, math.ceil(self.total_steps / self.steps_per_bar))

        label = self._default_event_label(midi)
        for s in range(target_steps):
            tok = tokens[s % n_tokens]
            if isinstance(tok, (int, float)):
                if tok > 0:
                    vel = float(tok) if tok <= 1.2 else velocity
                    self.events.append(NoteEvent(step=float(s), duration=1.0, midi=midi, velocity=vel, label=label))
                continue

            ch = str(tok)
            if ch in ("x", "1"):
                self.events.append(NoteEvent(step=float(s), duration=1.0, midi=midi, velocity=velocity, label=label))
            elif ch in ("X", "!"):
                self.events.append(
                    NoteEvent(step=float(s), duration=1.0, midi=midi, velocity=min(1.2, velocity * 1.18), label=label)
                )
            elif ch in ("o", "g"):
                self.events.append(
                    NoteEvent(step=float(s), duration=0.8, midi=midi, velocity=velocity * 0.48, label=label)
                )
            elif ch in ("r", "*"):
                self.events.append(
                    NoteEvent(step=float(s), duration=0.5, midi=midi, velocity=velocity * 0.78, label=label)
                )
                self.events.append(
                    NoteEvent(step=float(s) + 0.5, duration=0.5, midi=midi, velocity=velocity * 0.92, label=label)
                )
            elif ch == "t":
                for sub in range(3):
                    self.events.append(
                        NoteEvent(
                            step=float(s) + sub / 3.0,
                            duration=1.0 / 3.0,
                            midi=midi,
                            velocity=velocity * (0.65 + 0.15 * sub),
                            label=label,
                        )
                    )
            elif ch in ("-", "_") and self.events:
                self.events[-1].duration += 1.0
        return self

    def notes(
        self,
        seq: str | list[Any],
        step_size: float = 1.0,
        gate: float = 0.95,
        velocity: float = 0.85,
        repeat: bool = True,
    ) -> "Track":
        """Schedule pitched notes from a whitespace-separated string or list.

        Examples:
        - "C2 . . C2 Eb2 . G2 ."
        - "C4:2 Eb4:2 G4:4" (with explicit step durations after ':')
        - "C2->G2" (pitch slide/glide from C2 to G2, ideal for 808s and Acid 303!)
        - [(0, "C4", 2), (2, "Eb4", 2), (4, "G4", 4)]
        """
        self._invalidate()
        if isinstance(seq, str):
            self._notes_str = seq.strip()
            tokens = [t for t in seq.replace("|", " ").split() if t]
        else:
            tokens = list(seq)
            self._notes_str = " ".join(str(t) for t in tokens)

        if not tokens:
            return self

        # Check if list of tuple specs: (step, note, duration[, velocity])
        if isinstance(tokens[0], (tuple, list)) and len(tokens[0]) >= 2 and isinstance(tokens[0][0], (int, float)):
            for item in tokens:
                st = float(item[0])
                note_spec = item[1]
                dur = float(item[2]) if len(item) >= 3 else step_size
                vel = float(item[3]) if len(item) >= 4 else velocity
                midi, slide_to = self._parse_note_slide(note_spec)
                self.events.append(
                    NoteEvent(
                        step=st,
                        duration=dur * gate,
                        midi=midi,
                        velocity=vel,
                        label=midi_to_note(midi),
                        slide_to=slide_to,
                    )
                )
            return self

        parsed: list[tuple[str, float]] = []
        for tok in tokens:
            s = str(tok).strip()
            dur = float(step_size)
            if ":" in s:
                base, dur_s = s.rsplit(":", 1)
                try:
                    dur = float(dur_s)
                    s = base
                except ValueError:
                    pass
            parsed.append((s, dur))

        pattern_len_steps = sum(d for _, d in parsed)
        if pattern_len_steps <= 0:
            return self

        total_target = float(self.total_steps) if repeat else pattern_len_steps
        cursor = 0.0
        idx = 0
        while cursor < total_target - 1e-6:
            sym, dur = parsed[idx % len(parsed)]
            actual_dur = min(dur, total_target - cursor)
            if sym in (".", "r", "rest", "0"):
                pass
            elif sym in ("-", "_"):
                if self.events:
                    self.events[-1].duration += actual_dur
            else:
                midi, slide_to = self._parse_note_slide(sym)
                self.events.append(
                    NoteEvent(
                        step=cursor,
                        duration=max(0.15, actual_dur * gate),
                        midi=midi,
                        velocity=velocity,
                        label=midi_to_note(midi),
                        slide_to=slide_to,
                    )
                )
            cursor += dur
            idx += 1
            if not repeat and idx >= len(parsed):
                break

        return self

    def _parse_note_slide(self, spec: Any) -> tuple[int, int | None]:
        s = str(spec).strip()
        if "->" in s:
            a, b = s.split("->", 1)
            return note_to_midi(a, self.default_octave), note_to_midi(b, self.default_octave)
        if "/" in s and not s.startswith("/"):
            a, b = s.split("/", 1)
            return note_to_midi(a, self.default_octave), note_to_midi(b, self.default_octave)
        return note_to_midi(s, self.default_octave), None

    def chords(
        self,
        progression: list[Any] | str,
        steps_per_chord: float = 8.0,
        octave: int | None = None,
        velocity: float = 0.72,
        gate: float = 0.95,
    ) -> "Track":
        """Schedule a chord progression across the track.

        `progression` can be:
        - A string like `"Cm7 | Fm7 | Abmaj7 | G7"` or `"Cm7 Fm7 Abmaj7 G7"`
        - A list of chord names `["Cm7", "Fm7", "Abmaj7", "G7"]`
        - A list of explicit note lists `[["C4", "Eb4", "G4"], ["F3", "Ab3", "C4"]]`
        """
        self._invalidate()
        octv = octave if octave is not None else self.default_octave
        if isinstance(progression, str):
            raw_items = [c.strip() for c in progression.replace("|", " ").split() if c.strip()]
        else:
            raw_items = list(progression)

        if not raw_items:
            return self

        cursor = 0.0
        idx = 0
        while cursor < self.total_steps - 1e-6:
            spec = raw_items[idx % len(raw_items)]
            dur = float(steps_per_chord)
            if isinstance(spec, str) and ":" in spec:
                base, dur_s = spec.rsplit(":", 1)
                try:
                    dur = float(dur_s)
                    spec = base
                except ValueError:
                    pass
            actual_dur = min(dur, self.total_steps - cursor)
            pitches = chord_notes(spec, default_octave=octv)
            for p in pitches:
                self.events.append(
                    NoteEvent(
                        step=cursor,
                        duration=actual_dur * gate,
                        midi=p,
                        velocity=velocity,
                        label=midi_to_note(p),
                    )
                )
            cursor += dur
            idx += 1
        return self

    def arp(
        self,
        notes_or_chords: list[Any] | str,
        rate: float = 1.0,
        mode: str = "up",
        octaves: int = 1,
        gate: float = 0.75,
        velocity: float = 0.8,
    ) -> "Track":
        """Generate an arpeggiated sequence across `total_steps`.

        `rate` is the step length of each arpeggio note (1.0 = 16th note, 0.5 = 32nd note, 2.0 = 8th note).
        `mode` can be `'up'`, `'down'`, `'updown'`.
        """
        self._invalidate()
        if isinstance(notes_or_chords, str):
            parts = [p.strip() for p in notes_or_chords.replace("|", " ").split() if p.strip()]
            base_pitches: list[int] = []
            for p in parts:
                if any(ch.isdigit() for ch in p) and len(p) <= 3:
                    base_pitches.append(note_to_midi(p, self.default_octave))
                else:
                    base_pitches.extend(chord_notes(p, default_octave=self.default_octave))
        else:
            base_pitches = []
            for item in notes_or_chords:
                if isinstance(item, (int, float)) or (isinstance(item, str) and any(c.isdigit() for c in item)):
                    base_pitches.append(note_to_midi(item, self.default_octave))
                else:
                    base_pitches.extend(chord_notes(item, default_octave=self.default_octave))

        if not base_pitches:
            base_pitches = [60, 63, 67, 72]

        expanded: list[int] = []
        for o in range(max(1, int(octaves))):
            expanded.extend(p + 12 * o for p in base_pitches)

        if mode == "down":
            expanded = list(reversed(expanded))
        elif mode == "updown" and len(expanded) > 2:
            expanded = expanded + list(reversed(expanded[1:-1]))

        step_inc = max(0.25, float(rate))
        cursor = 0.0
        idx = 0
        while cursor < self.total_steps - 1e-6:
            midi = expanded[idx % len(expanded)]
            accent = 1.08 if (idx % 4 == 0) else 0.88
            self.events.append(
                NoteEvent(
                    step=cursor,
                    duration=step_inc * gate,
                    midi=midi,
                    velocity=min(1.15, velocity * accent),
                    label=midi_to_note(midi),
                )
            )
            cursor += step_inc
            idx += 1
        return self

    def hit(
        self,
        step: float,
        note: int | str | None = None,
        duration: float = 1.0,
        velocity: float = 0.9,
        slide_to: int | str | None = None,
    ) -> "Track":
        """Add a single note or drum hit at a specific 16th-note step."""
        self._invalidate()
        midi = note_to_midi(note, self.default_octave) if note is not None else self.base_midi
        slide_midi = note_to_midi(slide_to, self.default_octave) if slide_to is not None else None
        self.events.append(
            NoteEvent(
                step=float(step),
                duration=float(duration),
                midi=midi,
                velocity=float(velocity),
                label=self._default_event_label(midi),
                slide_to=slide_midi,
            )
        )
        return self

    def roll(
        self,
        step: float,
        count: int = 4,
        length: float = 1.0,
        note: int | str | None = None,
        velocity_start: float = 0.45,
        velocity_end: float = 0.98,
    ) -> "Track":
        """Insert a fast trap hi-hat or snare roll starting at `step` spanning `length` steps."""
        self._invalidate()
        n = max(1, int(count))
        sub_dur = float(length) / n
        midi = note_to_midi(note, self.default_octave) if note is not None else self.base_midi
        label = self._default_event_label(midi)
        for i in range(n):
            frac = i / max(1, n - 1)
            vel = velocity_start + (velocity_end - velocity_start) * frac
            self.events.append(
                NoteEvent(
                    step=float(step) + i * sub_dur,
                    duration=sub_dur * 0.9,
                    midi=midi,
                    velocity=vel,
                    label=label,
                )
            )
        return self

    def _default_event_label(self, midi: int) -> str:
        if self.instrument in ("kick", "snare", "clap", "hihat", "closed_hat", "open_hat", "perc", "rim", "tom", "cowbell"):
            return self.name
        return midi_to_note(midi)

    # ── Envelope & Effects Chain Methods ────────────────────────────────

    def adsr(
        self,
        attack: float | None = None,
        decay: float | None = None,
        sustain: float | None = None,
        release: float | None = None,
    ) -> "Track":
        """Configure the track's amplitude ADSR envelope (seconds for A/D/R, 0..1 for S)."""
        self._invalidate()
        if attack is not None:
            self.attack = max(0.0005, float(attack))
        if decay is not None:
            self.decay = max(0.005, float(decay))
        if sustain is not None:
            self.sustain = max(0.0, min(1.0, float(sustain)))
        if release is not None:
            self.release = max(0.005, float(release))
        return self

    def filter(
        self,
        cutoff: float = 1800.0,
        resonance: float = 0.25,
        kind: str = "lowpass",
    ) -> "Track":
        """Add a resonant lowpass, highpass, or bandpass filter to the track."""
        self._invalidate()
        self.effects.append(
            EffectSpec(
                kind="filter",
                params={
                    "cutoff": float(max(30.0, min(18000.0, cutoff))),
                    "resonance": float(max(0.0, min(0.95, resonance))),
                    "mode": str(kind).lower(),
                },
            )
        )
        return self

    def lowpass(self, cutoff: float = 1800.0, resonance: float = 0.25) -> "Track":
        """Add a lowpass filter to the track."""
        return self.filter(cutoff=cutoff, resonance=resonance, kind="lowpass")

    def highpass(self, cutoff: float = 250.0, resonance: float = 0.2) -> "Track":
        """Add a highpass filter to the track."""
        return self.filter(cutoff=cutoff, resonance=resonance, kind="highpass")

    def bandpass(self, cutoff: float = 1200.0, resonance: float = 0.4) -> "Track":
        """Add a bandpass filter to the track."""
        return self.filter(cutoff=cutoff, resonance=resonance, kind="bandpass")

    def drive(self, amount: float = 0.4, mix: float = 1.0) -> "Track":
        """Add warm soft-clipping / tube saturation distortion."""
        self._invalidate()
        self.effects.append(
            EffectSpec(
                kind="drive",
                params={
                    "amount": float(max(0.0, min(1.0, amount))),
                    "mix": float(max(0.0, min(1.0, mix))),
                },
            )
        )
        return self

    def distort(self, amount: float = 0.5, mix: float = 1.0) -> "Track":
        """Alias for `.drive(amount, mix)`."""
        return self.drive(amount=amount, mix=mix)

    def bitcrush(self, bits: int = 8, downsample: int = 3, mix: float = 0.8) -> "Track":
        """Add lo-fi bitcrusher / sample-rate reduction."""
        self._invalidate()
        self.effects.append(
            EffectSpec(
                kind="bitcrush",
                params={
                    "bits": int(max(2, min(16, bits))),
                    "downsample": int(max(1, min(32, downsample))),
                    "mix": float(max(0.0, min(1.0, mix))),
                },
            )
        )
        return self

    def delay(
        self,
        time_steps: float = 3.0,
        feedback: float = 0.35,
        mix: float = 0.28,
    ) -> "Track":
        """Add tempo-synced echo/delay (`time_steps` in 16th-note steps, e.g. 3.0 = dotted 8th)."""
        self._invalidate()
        self.effects.append(
            EffectSpec(
                kind="delay",
                params={
                    "time_steps": float(max(0.25, min(16.0, time_steps))),
                    "feedback": float(max(0.0, min(0.9, feedback))),
                    "mix": float(max(0.0, min(1.0, mix))),
                },
            )
        )
        return self

    def reverb(
        self,
        room: float = 0.65,
        damp: float = 0.4,
        mix: float = 0.25,
    ) -> "Track":
        """Add algorithmic Schroeder/comb-allpass stereo-style room reverb."""
        self._invalidate()
        self.effects.append(
            EffectSpec(
                kind="reverb",
                params={
                    "room": float(max(0.1, min(0.98, room))),
                    "damp": float(max(0.0, min(0.95, damp))),
                    "mix": float(max(0.0, min(1.0, mix))),
                },
            )
        )
        return self

    def chorus(self, depth: float = 0.0025, rate: float = 1.6, mix: float = 0.35) -> "Track":
        """Add modulated chorus widening effect."""
        self._invalidate()
        self.effects.append(
            EffectSpec(
                kind="chorus",
                params={
                    "depth": float(max(0.0002, min(0.01, depth))),
                    "rate": float(max(0.1, min(10.0, rate))),
                    "mix": float(max(0.0, min(1.0, mix))),
                },
            )
        )
        return self

    def sidechain(
        self,
        amount: float = 0.72,
        period_steps: float = 4.0,
        attack_Ratio: float = 0.12,
    ) -> "Track":
        """Add rhythmic sidechain ducking (e.g. every 4 steps = quarter-note kick pumping)."""
        self._invalidate()
        self.effects.append(
            EffectSpec(
                kind="sidechain",
                params={
                    "amount": float(max(0.0, min(1.0, amount))),
                    "period_steps": float(max(1.0, period_steps)),
                    "attack_ratio": float(max(0.02, min(0.5, attack_Ratio))),
                },
            )
        )
        return self

    def gain(self, volume: float) -> "Track":
        """Set track output volume (0.0 to 1.5)."""
        self._invalidate()
        self.volume = _FluentFloat(max(0.0, min(1.5, volume)), self, "volume")
        return self

    def set_pan(self, pan: float) -> "Track":
        """Set stereo pan (-1.0 = full left, 0.0 = center, +1.0 = full right)."""
        self.pan = _FluentFloat(max(-1.0, min(1.0, pan)), self, "pan")
        return self

    def transpose(self, semitones: int) -> "Track":
        """Transpose all scheduled pitched events by `semitones`."""
        self._invalidate()
        shift = int(semitones)
        self.base_midi = max(12, min(120, self.base_midi + shift))
        for ev in self.events:
            ev.midi = max(12, min(120, ev.midi + shift))
            if ev.slide_to is not None:
                ev.slide_to = max(12, min(120, ev.slide_to + shift))
            ev.label = self._default_event_label(ev.midi)
        return self

    # ── Synthesis & Audio Rendering ─────────────────────────────────────

    def render(self, sample_rate: int = SAMPLE_RATE, apply_volume: bool = True) -> np.ndarray:
        """Synthesize the complete track audio buffer as a float32 NumPy array in [-1.0, 1.0]."""
        if self._rendered_audio is not None and sample_rate == SAMPLE_RATE and apply_volume:
            return self._rendered_audio

        step_sec = self.step_duration
        total_sec = max(0.25, self.total_steps * step_sec)
        n_samples = int(round(total_sec * sample_rate))
        buf = np.zeros(n_samples, dtype=np.float32)

        # Render each scheduled event
        for ev in self.events:
            # Apply swing to odd 16th steps
            step_pos = ev.step
            step_int = int(round(step_pos))
            if abs(step_pos - step_int) < 1e-3 and (step_int % 2 == 1) and self.swing > 0:
                step_pos += self.swing * 0.5

            start_sec = step_pos * step_sec
            start_idx = int(round(start_sec * sample_rate))
            if start_idx >= n_samples:
                continue

            dur_sec = max(0.02, ev.duration * step_sec)
            note_audio = self._synthesize_event(ev, dur_sec, sample_rate)
            if note_audio.size == 0:
                continue

            # Mix with wrap-around tail so loops sound seamless
            end_idx = start_idx + note_audio.size
            if end_idx <= n_samples:
                buf[start_idx:end_idx] += note_audio
            else:
                first_part = n_samples - start_idx
                buf[start_idx:n_samples] += note_audio[:first_part]
                rem = min(n_samples, note_audio.size - first_part)
                if rem > 0:
                    buf[:rem] += note_audio[first_part : first_part + rem] * 0.65

        # Apply DSP effects chain
        for fx in self.effects:
            buf = self._apply_effect(buf, fx, sample_rate, step_sec)

        if apply_volume:
            buf = buf * float(self.volume)

        # Gentle master limiting per track to avoid harsh digital clipping
        peak = float(np.max(np.abs(buf))) if buf.size else 0.0
        if peak > 0.98:
            buf = np.tanh(buf * 1.1) * 0.95

        buf = buf.astype(np.float32)
        if sample_rate == SAMPLE_RATE and apply_volume:
            self._rendered_audio = buf
        return buf

    def _synthesize_event(self, ev: NoteEvent, dur_sec: float, sr: int) -> np.ndarray:
        inst = self.instrument
        vel = float(ev.velocity)
        freq = midi_to_freq(ev.midi)
        slide_freq = midi_to_freq(ev.slide_to) if ev.slide_to is not None else None

        if inst == "kick":
            return _synth_kick(freq, self.decay, vel, sr)
        if inst == "snare":
            return _synth_snare(freq, self.decay, vel, sr)
        if inst == "clap":
            return _synth_clap(self.decay, vel, sr)
        if inst in ("hihat", "closed_hat"):
            return _synth_hihat(self.decay, vel, sr, open_hat=False)
        if inst == "open_hat":
            return _synth_hihat(max(0.18, self.decay), vel, sr, open_hat=True)
        if inst in ("perc", "rim"):
            return _synth_rim_perc(freq, self.decay, vel, sr)
        if inst == "tom":
            return _synth_tom(freq, self.decay, vel, sr)
        if inst == "cowbell":
            return _synth_cowbell(self.decay, vel, sr)
        if inst in ("bass808", "808", "sub"):
            return _synth_808(
                freq,
                slide_freq,
                dur_sec,
                self.attack,
                self.decay,
                self.sustain,
                self.release,
                vel,
                sr,
                sub_only=(inst == "sub"),
            )
        if inst == "acid":
            return _synth_acid(
                freq,
                slide_freq,
                dur_sec,
                self.attack,
                self.decay,
                self.sustain,
                self.release,
                vel,
                sr,
            )
        if inst == "pluck":
            return _synth_pluck(freq, dur_sec, self.decay, self.release, vel, sr)
        if inst in ("bell", "fm"):
            return _synth_fm_bell(
                freq,
                dur_sec,
                self.attack,
                self.decay,
                self.sustain,
                self.release,
                vel,
                sr,
                bright=(inst == "bell"),
            )
        if inst in ("epiano", "keys"):
            return _synth_epiano(
                freq,
                dur_sec,
                self.attack,
                self.decay,
                self.sustain,
                self.release,
                vel,
                sr,
            )

        # Standard oscillator synths: saw, supersaw, square, pulse, sine, triangle, pad, arp
        return _synth_osc(
            inst,
            freq,
            slide_freq,
            dur_sec,
            self.attack,
            self.decay,
            self.sustain,
            self.release,
            vel,
            sr,
        )

    def _apply_effect(
        self, buf: np.ndarray, fx: EffectSpec, sr: int, step_sec: float
    ) -> np.ndarray:
        kind = fx.kind
        p = fx.params
        if kind == "filter":
            return _fx_filter(
                buf,
                cutoff=float(p.get("cutoff", 1800.0)),
                resonance=float(p.get("resonance", 0.25)),
                mode=str(p.get("mode", "lowpass")),
                sr=sr,
            )
        if kind == "drive":
            amount = float(p.get("amount", 0.4))
            mix = float(p.get("mix", 1.0))
            gain = 1.0 + amount * 8.0
            wet = np.tanh(buf * gain) / math.tanh(gain)
            return (1.0 - mix) * buf + mix * wet
        if kind == "bitcrush":
            bits = int(p.get("bits", 8))
            downsample = int(p.get("downsample", 3))
            mix = float(p.get("mix", 0.8))
            levels = float(2 ** (bits - 1))
            wet = np.round(buf * levels) / levels
            if downsample > 1:
                idx = (np.arange(wet.size) // downsample) * downsample
                idx = np.clip(idx, 0, wet.size - 1)
                wet = wet[idx]
            return (1.0 - mix) * buf + mix * wet
        if kind == "delay":
            time_steps = float(p.get("time_steps", 3.0))
            feedback = float(p.get("feedback", 0.35))
            mix = float(p.get("mix", 0.28))
            delay_samples = max(1, int(round(time_steps * step_sec * sr)))
            wet = np.zeros_like(buf)
            cur = buf.copy()
            for tap in range(1, 5):
                shift = tap * delay_samples
                gain = feedback ** tap
                if gain < 0.02:
                    break
                rolled = np.roll(cur, shift)
                wet += rolled * gain
            return buf + mix * wet
        if kind == "reverb":
            room = float(p.get("room", 0.65))
            damp = float(p.get("damp", 0.4))
            mix = float(p.get("mix", 0.25))
            return _fx_reverb(buf, room=room, damp=damp, mix=mix, sr=sr)
        if kind == "chorus":
            depth = float(p.get("depth", 0.0025))
            rate = float(p.get("rate", 1.6))
            mix = float(p.get("mix", 0.35))
            n = buf.size
            t = np.arange(n, dtype=np.float32) / sr
            mod = (0.012 + depth * np.sin(2.0 * np.pi * rate * t)) * sr
            src_idx = (np.arange(n, dtype=np.float32) - mod) % n
            i0 = src_idx.astype(np.int32)
            i1 = (i0 + 1) % n
            frac = src_idx - i0
            wet = (1.0 - frac) * buf[i0] + frac * buf[i1]
            return (1.0 - mix) * buf + mix * wet
        if kind == "sidechain":
            amount = float(p.get("amount", 0.72))
            period_steps = float(p.get("period_steps", 4.0))
            attack_ratio = float(p.get("attack_ratio", 0.12))
            period_sec = max(0.05, period_steps * step_sec)
            t = np.arange(buf.size, dtype=np.float32) / sr
            phase = (t % period_sec) / period_sec
            env = np.where(
                phase < attack_ratio,
                phase / max(1e-4, attack_ratio),
                1.0 - np.exp(-4.5 * (phase - attack_ratio) / max(1e-4, 1.0 - attack_ratio)),
            )
            gain_curve = (1.0 - amount) + amount * env
            return buf * gain_curve.astype(np.float32)
        return buf

    # ── Analytics & Serialization Helpers ───────────────────────────────

    def compute_waveform_peaks(self, bins: int = 256) -> list[list[float]]:
        """Downsample rendered audio into `[min, max]` envelope pairs for fast GUI rendering."""
        audio = self.render()
        if audio.size == 0:
            return [[0.0, 0.0]] * bins
        n = audio.size
        step = max(1, n // bins)
        trimmed = audio[: step * bins]
        if trimmed.size < step * bins:
            padded = np.zeros(step * bins, dtype=np.float32)
            padded[: audio.size] = audio
            trimmed = padded
        reshaped = trimmed.reshape(bins, step)
        mins = np.min(reshaped, axis=1)
        maxs = np.max(reshaped, axis=1)
        return [[round(float(mn), 4), round(float(mx), 4)] for mn, mx in zip(mins, maxs)]

    def compute_spectrum_bands(self, num_bands: int = 16) -> list[float]:
        """Compute normalized log-spaced frequency band energies (0.0 .. 1.0) across the track."""
        audio = self.render()
        if audio.size < 64:
            return [0.0] * num_bands
        # Use up to 65536 samples for fast FFT
        seg = audio[: min(audio.size, 65536)]
        window = np.hanning(seg.size)
        spec = np.abs(np.fft.rfft(seg * window))
        freqs = np.fft.rfftfreq(seg.size, d=1.0 / SAMPLE_RATE)

        edges = np.geomspace(30.0, 14000.0, num_bands + 1)
        bands: list[float] = []
        for i in range(num_bands):
            mask = (freqs >= edges[i]) & (freqs < edges[i + 1])
            if np.any(mask):
                val = float(np.sqrt(np.mean(spec[mask] ** 2)))
            else:
                val = 0.0
            bands.append(val)

        max_val = max(bands) if bands else 0.0
        if max_val > 1e-6:
            bands = [round(min(1.0, (b / max_val) ** 0.65), 4) for b in bands]
        else:
            bands = [0.0] * num_bands
        return bands

    def encode_pcm16_base64(self, sample_rate: int = SAMPLE_RATE) -> str:
        """Encode the un-panned mono float32 waveform (at unit track volume) as base64 int16 PCM."""
        raw = self.render(sample_rate=sample_rate, apply_volume=False)
        clipped = np.clip(raw, -1.0, 1.0)
        pcm16 = (clipped * 32767.0).astype("<i2")
        return base64.b64encode(pcm16.tobytes()).decode("ascii")

    def metrics(self) -> dict[str, Any]:
        """Return detailed audio and musical metrics for the Audio Analyzer panel."""
        audio = self.render()
        peak = float(np.max(np.abs(audio))) if audio.size else 0.0
        rms = float(np.sqrt(np.mean(audio ** 2))) if audio.size else 0.0
        peak_db = 20.0 * math.log10(max(1e-5, peak))
        rms_db = 20.0 * math.log10(max(1e-5, rms))
        crest_db = max(0.0, peak_db - rms_db)

        bands = self.compute_spectrum_bands(16)
        sub_energy = float(np.mean(bands[0:3])) if len(bands) >= 3 else 0.0
        bass_energy = float(np.mean(bands[3:6])) if len(bands) >= 6 else 0.0
        mid_energy = float(np.mean(bands[6:11])) if len(bands) >= 11 else 0.0
        high_energy = float(np.mean(bands[11:16])) if len(bands) >= 16 else 0.0

        return {
            "peak": round(peak, 4),
            "rms": round(rms, 4),
            "peak_db": round(peak_db, 2),
            "rms_db": round(rms_db, 2),
            "crest_db": round(crest_db, 2),
            "bpm": round(self.bpm, 2),
            "bars": self.bars,
            "steps": self.total_steps,
            "duration": round(self.duration, 3),
            "note_count": len(self.events),
            "sub_energy": round(sub_energy, 3),
            "bass_energy": round(bass_energy, 3),
            "mid_energy": round(mid_energy, 3),
            "high_energy": round(high_energy, 3),
        }


class Beat:
    """High-level multi-track Beat arranger & studio session container."""

    def __init__(
        self,
        name: str = "Beat",
        bpm: float = 120.0,
        bars: int = 2,
        steps_per_bar: int = 16,
        swing: float = 0.0,
        key: str = "C",
        scale: str = "minor",
        master_volume: float = 0.9,
    ) -> None:
        self.name = str(name)
        self.bpm = set_bpm(bpm)
        self.bars = set_bars(bars, steps_per_bar)
        self.steps_per_bar = int(steps_per_bar)
        self.swing = set_swing(swing)
        self.key = str(key)
        self.scale = str(scale)
        self.master_volume = float(max(0.0, min(1.5, master_volume)))
        self.tracks: list[Track] = []

    @property
    def total_steps(self) -> int:
        if not self.tracks:
            return self.bars * self.steps_per_bar
        return max(t.total_steps for t in self.tracks)

    @property
    def duration(self) -> float:
        if not self.tracks:
            return self.total_steps * ((60.0 / max(30.0, self.bpm)) / 4.0)
        return max(t.duration for t in self.tracks)

    def add(self, track: Track) -> Track:
        """Attach an existing Track to this Beat."""
        if track not in self.tracks:
            self.tracks.append(track)
        return track

    def track(
        self,
        name: str = "Track",
        instrument: str = "synth",
        pattern: str | list[Any] | None = None,
        notes: str | list[Any] | None = None,
        chords: list[Any] | None = None,
        **kwargs: Any,
    ) -> Track:
        """Create and register a new Track inside this Beat."""
        kwargs.setdefault("bpm", self.bpm)
        kwargs.setdefault("bars", self.bars)
        kwargs.setdefault("steps_per_bar", self.steps_per_bar)
        kwargs.setdefault("swing", self.swing)
        tr = Track(
            name=name,
            instrument=instrument,
            pattern=pattern,
            notes=notes,
            chords=chords,
            **kwargs,
        )
        self.tracks.append(tr)
        return tr

    def kick(self, pattern: str | None = "x...x...x...x...", name: str = "Kick", **kwargs: Any) -> Track:
        return self.track(name=name, instrument="kick", pattern=pattern, **kwargs)

    def snare(self, pattern: str | None = "....x.......x...", name: str = "Snare", **kwargs: Any) -> Track:
        return self.track(name=name, instrument="snare", pattern=pattern, **kwargs)

    def clap(self, pattern: str | None = "....x.......x...", name: str = "Clap", **kwargs: Any) -> Track:
        return self.track(name=name, instrument="clap", pattern=pattern, **kwargs)

    def hihat(self, pattern: str | None = "x.x.x.x.x.x.x.x.", name: str = "Hi-Hat", **kwargs: Any) -> Track:
        return self.track(name=name, instrument="hihat", pattern=pattern, **kwargs)

    def open_hat(self, pattern: str | None = "..x...x...x...x.", name: str = "Open Hat", **kwargs: Any) -> Track:
        return self.track(name=name, instrument="open_hat", pattern=pattern, **kwargs)

    def perc(self, pattern: str | None = "...x..x....x..x.", name: str = "Perc", **kwargs: Any) -> Track:
        return self.track(name=name, instrument="perc", pattern=pattern, **kwargs)

    def tom(self, pattern: str | None = None, notes: str | None = None, name: str = "Tom", **kwargs: Any) -> Track:
        return self.track(name=name, instrument="tom", pattern=pattern, notes=notes, **kwargs)

    def cowbell(self, pattern: str | None = "x..x..x.x...x...", name: str = "Cowbell", **kwargs: Any) -> Track:
        return self.track(name=name, instrument="cowbell", pattern=pattern, **kwargs)

    def bass808(
        self,
        notes: str | list[Any] | None = None,
        pattern: str | None = None,
        name: str = "808 Bass",
        **kwargs: Any,
    ) -> Track:
        kwargs.setdefault("octave", 2)
        return self.track(name=name, instrument="bass808", notes=notes, pattern=pattern, **kwargs)

    def sub(
        self,
        notes: str | list[Any] | None = None,
        pattern: str | None = None,
        name: str = "Sub Bass",
        **kwargs: Any,
    ) -> Track:
        kwargs.setdefault("octave", 2)
        return self.track(name=name, instrument="sub", notes=notes, pattern=pattern, **kwargs)

    def synth(
        self,
        notes: str | list[Any] | None = None,
        name: str = "Synth Lead",
        instrument: str = "saw",
        **kwargs: Any,
    ) -> Track:
        return self.track(name=name, instrument=instrument, notes=notes, **kwargs)

    def pluck(
        self,
        notes: str | list[Any] | None = None,
        name: str = "Pluck",
        **kwargs: Any,
    ) -> Track:
        return self.track(name=name, instrument="pluck", notes=notes, **kwargs)

    def pad(
        self,
        chords: list[Any] | str | None = None,
        notes: str | list[Any] | None = None,
        name: str = "Warm Pad",
        **kwargs: Any,
    ) -> Track:
        return self.track(name=name, instrument="pad", chords=chords, notes=notes, **kwargs)

    def keys(
        self,
        chords: list[Any] | str | None = None,
        notes: str | list[Any] | None = None,
        name: str = "Electric Piano",
        **kwargs: Any,
    ) -> Track:
        return self.track(name=name, instrument="epiano", chords=chords, notes=notes, **kwargs)

    def acid(
        self,
        notes: str | list[Any] | None = None,
        name: str = "Acid 303",
        **kwargs: Any,
    ) -> Track:
        kwargs.setdefault("octave", 2)
        return self.track(name=name, instrument="acid", notes=notes, **kwargs)

    def bell(
        self,
        notes: str | list[Any] | None = None,
        name: str = "Dark Bell",
        **kwargs: Any,
    ) -> Track:
        kwargs.setdefault("octave", 5)
        return self.track(name=name, instrument="bell", notes=notes, **kwargs)

    def arp(
        self,
        notes_or_chords: list[Any] | str,
        name: str = "Arpeggio",
        rate: float = 1.0,
        mode: str = "up",
        octaves: int = 1,
        instrument: str = "pluck",
        **kwargs: Any,
    ) -> Track:
        tr = self.track(name=name, instrument=instrument, **kwargs)
        tr.arp(notes_or_chords, rate=rate, mode=mode, octaves=octaves)
        return tr

    def render_stereo(self, sample_rate: int = SAMPLE_RATE) -> np.ndarray:
        """Mix all active tracks into a stereo float32 array of shape `(N, 2)`."""
        return mix_tracks_stereo(self.tracks, sample_rate=sample_rate, master_volume=self.master_volume)


# ── Convenience Top-Level Track Constructors ────────────────────────────


def kick(pattern: str | None = "x...x...x...x...", name: str = "Kick", **kwargs: Any) -> Track:
    return Track(name=name, instrument="kick", pattern=pattern, **kwargs)


def snare(pattern: str | None = "....x.......x...", name: str = "Snare", **kwargs: Any) -> Track:
    return Track(name=name, instrument="snare", pattern=pattern, **kwargs)


def clap(pattern: str | None = "....x.......x...", name: str = "Clap", **kwargs: Any) -> Track:
    return Track(name=name, instrument="clap", pattern=pattern, **kwargs)


def hihat(pattern: str | None = "x.x.x.x.x.x.x.x.", name: str = "Hi-Hat", **kwargs: Any) -> Track:
    return Track(name=name, instrument="hihat", pattern=pattern, **kwargs)


def open_hat(pattern: str | None = "..x...x...x...x.", name: str = "Open Hat", **kwargs: Any) -> Track:
    return Track(name=name, instrument="open_hat", pattern=pattern, **kwargs)


def perc(pattern: str | None = "...x..x....x..x.", name: str = "Perc", **kwargs: Any) -> Track:
    return Track(name=name, instrument="perc", pattern=pattern, **kwargs)


def bass808(notes: str | list[Any] | None = None, pattern: str | None = None, name: str = "808 Bass", **kwargs: Any) -> Track:
    kwargs.setdefault("octave", 2)
    return Track(name=name, instrument="bass808", notes=notes, pattern=pattern, **kwargs)


def sub(notes: str | list[Any] | None = None, pattern: str | None = None, name: str = "Sub Bass", **kwargs: Any) -> Track:
    kwargs.setdefault("octave", 2)
    return Track(name=name, instrument="sub", notes=notes, pattern=pattern, **kwargs)


def synth(notes: str | list[Any] | None = None, name: str = "Synth Lead", instrument: str = "saw", **kwargs: Any) -> Track:
    return Track(name=name, instrument=instrument, notes=notes, **kwargs)


def pluck(notes: str | list[Any] | None = None, name: str = "Pluck", **kwargs: Any) -> Track:
    return Track(name=name, instrument="pluck", notes=notes, **kwargs)


def pad(chords: list[Any] | str | None = None, notes: str | list[Any] | None = None, name: str = "Warm Pad", **kwargs: Any) -> Track:
    return Track(name=name, instrument="pad", chords=chords, notes=notes, **kwargs)


def keys(chords: list[Any] | str | None = None, notes: str | list[Any] | None = None, name: str = "Electric Piano", **kwargs: Any) -> Track:
    return Track(name=name, instrument="epiano", chords=chords, notes=notes, **kwargs)


def acid(notes: str | list[Any] | None = None, name: str = "Acid 303", **kwargs: Any) -> Track:
    kwargs.setdefault("octave", 2)
    return Track(name=name, instrument="acid", notes=notes, **kwargs)


def bell(notes: str | list[Any] | None = None, name: str = "Dark Bell", **kwargs: Any) -> Track:
    kwargs.setdefault("octave", 5)
    return Track(name=name, instrument="bell", notes=notes, **kwargs)


def get_registered_tracks() -> list[Track]:
    """Return all Tracks instantiated during the current script execution."""
    return list(_REGISTERED_TRACKS)


# ── Low-Level NumPy Synthesizers ────────────────────────────────────────


def _adsr_envelope(
    n_samples: int,
    note_samples: int,
    attack: float,
    decay: float,
    sustain: float,
    release: float,
    sr: int,
) -> np.ndarray:
    a_s = max(1, int(round(attack * sr)))
    d_s = max(1, int(round(decay * sr)))
    r_s = max(1, int(round(release * sr)))

    env = np.zeros(n_samples, dtype=np.float32)
    # Attack
    a_end = min(n_samples, a_s)
    env[:a_end] = np.linspace(0.0, 1.0, a_end, endpoint=False, dtype=np.float32)
    # Decay
    d_end = min(n_samples, a_end + d_s)
    if d_end > a_end:
        env[a_end:d_end] = np.linspace(1.0, sustain, d_end - a_end, endpoint=False, dtype=np.float32)
    # Sustain
    s_end = min(n_samples, max(d_end, note_samples))
    if s_end > d_end:
        env[d_end:s_end] = sustain
    # Release
    rel_end = min(n_samples, s_end + r_s)
    if rel_end > s_end:
        start_val = float(env[s_end - 1]) if s_end > 0 else sustain
        env[s_end:rel_end] = start_val * np.exp(-np.linspace(0.0, 5.0, rel_end - s_end, dtype=np.float32))
    return env


def _synth_kick(base_freq: float, decay: float, vel: float, sr: int) -> np.ndarray:
    dur = max(0.08, min(1.0, decay * 1.4))
    n = int(round(dur * sr))
    t = np.arange(n, dtype=np.float32) / sr
    f_end = max(38.0, min(72.0, base_freq * 0.72))
    f_start = f_end * 3.6
    # Fast exponential pitch drop
    freq_t = f_end + (f_start - f_end) * np.exp(-t * 36.0)
    phase = 2.0 * np.pi * np.cumsum(freq_t) / sr
    body = np.sin(phase)
    # Sub harmonic warmth + punch transient click
    click = np.sin(2.0 * np.pi * 1400.0 * t) * np.exp(-t * 180.0) * 0.35
    amp_env = np.exp(-t / max(0.04, decay * 0.65))
    sig = (body * amp_env + click) * vel * 1.15
    return np.tanh(sig * 1.35).astype(np.float32)


def _synth_snare(base_freq: float, decay: float, vel: float, sr: int) -> np.ndarray:
    dur = max(0.06, min(0.8, decay * 1.3))
    n = int(round(dur * sr))
    t = np.arange(n, dtype=np.float32) / sr
    # Dual drum membrane modes
    f1 = max(140.0, min(260.0, base_freq * 2.4))
    f2 = f1 * 1.58
    tone = (np.sin(2.0 * np.pi * f1 * t) * 0.65 + np.sin(2.0 * np.pi * f2 * t) * 0.35) * np.exp(-t * 28.0)

    # Deterministic pseudo-random crisp snare wires
    rng = np.random.default_rng(seed=104729)
    noise = rng.standard_normal(n).astype(np.float32)
    # Highpass the noise via difference
    hp_noise = np.empty_like(noise)
    hp_noise[0] = noise[0]
    hp_noise[1:] = noise[1:] - 0.68 * noise[:-1]
    noise_env = np.exp(-t / max(0.03, decay * 0.55))

    sig = (tone * 0.55 + hp_noise * noise_env * 0.65) * vel
    return np.tanh(sig * 1.3).astype(np.float32)


def _synth_clap(decay: float, vel: float, sr: int) -> np.ndarray:
    dur = max(0.08, min(0.8, decay * 1.4))
    n = int(round(dur * sr))
    t = np.arange(n, dtype=np.float32) / sr
    rng = np.random.default_rng(seed=224737)
    noise = rng.standard_normal(n).astype(np.float32)
    # Bandpass around 1.2 kHz
    bp = _fx_filter(noise, cutoff=1350.0, resonance=0.45, mode="bandpass", sr=sr)

    # 3 micro-slaps spaced by ~10ms followed by tail
    env = np.zeros(n, dtype=np.float32)
    for offset in (0.0, 0.009, 0.019):
        mask = t >= offset
        dt = t[mask] - offset
        env[mask] += np.exp(-dt * 160.0) * 0.75
    tail_mask = t >= 0.025
    env[tail_mask] += np.exp(-(t[tail_mask] - 0.025) / max(0.035, decay * 0.5)) * 0.55
    sig = bp * env * vel * 1.25
    return np.tanh(sig * 1.4).astype(np.float32)


def _synth_hihat(decay: float, vel: float, sr: int, open_hat: bool = False) -> np.ndarray:
    dur = max(0.03, min(1.0, decay * (1.8 if open_hat else 1.2)))
    n = int(round(dur * sr))
    t = np.arange(n, dtype=np.float32) / sr
    # 808-style 6 metallic square oscillators + crisp high noise
    ratios = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0]
    metal = np.zeros(n, dtype=np.float32)
    for r in ratios:
        metal += np.sign(np.sin(2.0 * np.pi * (r * 6.5) * t))
    metal /= len(ratios)

    rng = np.random.default_rng(seed=350377)
    noise = rng.standard_normal(n).astype(np.float32)
    raw = metal * 0.55 + noise * 0.45
    hp = _fx_filter(raw, cutoff=6800.0, resonance=0.2, mode="highpass", sr=sr)
    env = np.exp(-t / max(0.015, decay * (0.85 if open_hat else 0.45)))
    return (hp * env * vel * 0.9).astype(np.float32)


def _synth_rim_perc(freq: float, decay: float, vel: float, sr: int) -> np.ndarray:
    dur = max(0.03, min(0.4, decay * 1.2))
    n = int(round(dur * sr))
    t = np.arange(n, dtype=np.float32) / sr
    f1 = max(320.0, min(1200.0, freq * 4.2))
    f2 = f1 * 2.15
    sig = (np.sin(2.0 * np.pi * f1 * t) * 0.7 + np.sin(2.0 * np.pi * f2 * t) * 0.45) * np.exp(-t / max(0.015, decay * 0.45))
    return np.tanh(sig * vel * 1.4).astype(np.float32)


def _synth_tom(freq: float, decay: float, vel: float, sr: int) -> np.ndarray:
    dur = max(0.08, min(0.7, decay * 1.3))
    n = int(round(dur * sr))
    t = np.arange(n, dtype=np.float32) / sr
    f_base = max(75.0, min(280.0, freq))
    freq_t = f_base * (1.0 + 0.85 * np.exp(-t * 22.0))
    phase = 2.0 * np.pi * np.cumsum(freq_t) / sr
    env = np.exp(-t / max(0.04, decay * 0.6))
    return (np.sin(phase) * env * vel).astype(np.float32)


def _synth_cowbell(decay: float, vel: float, sr: int) -> np.ndarray:
    dur = max(0.06, min(0.6, decay * 1.4))
    n = int(round(dur * sr))
    t = np.arange(n, dtype=np.float32) / sr
    # Classic 808 Phonk cowbell frequencies: 540 Hz and 800 Hz
    sq1 = np.tanh(np.sin(2.0 * np.pi * 540.0 * t) * 3.5)
    sq2 = np.tanh(np.sin(2.0 * np.pi * 800.0 * t) * 3.5)
    env = np.exp(-t / max(0.03, decay * 0.65))
    bp = _fx_filter((sq1 + sq2) * 0.5, cutoff=1100.0, resonance=0.35, mode="bandpass", sr=sr)
    return (bp * env * vel * 1.2).astype(np.float32)


def _synth_808(
    freq: float,
    slide_freq: float | None,
    dur_sec: float,
    attack: float,
    decay: float,
    sustain: float,
    release: float,
    vel: float,
    sr: int,
    sub_only: bool = False,
) -> np.ndarray:
    note_samples = max(1, int(round(dur_sec * sr)))
    total_samples = note_samples + int(round(release * sr))
    t = np.arange(total_samples, dtype=np.float32) / sr

    if slide_freq is not None:
        glide = np.clip(t / max(0.04, dur_sec * 0.75), 0.0, 1.0)
        freq_curve = freq + (slide_freq - freq) * (glide ** 1.5)
    else:
        freq_curve = np.full(total_samples, freq, dtype=np.float32)

    # Punchy initial pitch thump on 808
    if not sub_only:
        freq_curve = freq_curve * (1.0 + 1.4 * np.exp(-t * 48.0))

    phase = 2.0 * np.pi * np.cumsum(freq_curve) / sr
    fundamental = np.sin(phase)
    if sub_only:
        wave_sig = fundamental
    else:
        harm2 = np.sin(2.0 * phase) * 0.28 * np.exp(-t * 4.0)
        harm3 = np.sin(3.0 * phase) * 0.10 * np.exp(-t * 7.0)
        wave_sig = np.tanh((fundamental + harm2 + harm3) * 1.45)

    env = _adsr_envelope(total_samples, note_samples, attack, decay, sustain, release, sr)
    return (wave_sig * env * vel).astype(np.float32)


def _synth_acid(
    freq: float,
    slide_freq: float | None,
    dur_sec: float,
    attack: float,
    decay: float,
    sustain: float,
    release: float,
    vel: float,
    sr: int,
) -> np.ndarray:
    note_samples = max(1, int(round(dur_sec * sr)))
    total_samples = note_samples + int(round(release * sr))
    t = np.arange(total_samples, dtype=np.float32) / sr

    if slide_freq is not None:
        glide = np.clip(t / max(0.03, dur_sec * 0.65), 0.0, 1.0)
        freq_curve = freq + (slide_freq - freq) * glide
    else:
        freq_curve = np.full(total_samples, freq, dtype=np.float32)

    phase = (np.cumsum(freq_curve) / sr) % 1.0
    saw = 2.0 * phase - 1.0
    sq = np.where(phase < 0.5, 1.0, -1.0)
    raw = (0.7 * saw + 0.3 * sq).astype(np.float32)

    # TB-303 resonant filter envelope sweep
    cutoff_hi = min(8500.0, freq * (8.0 + 5.0 * vel))
    cutoff_lo = max(140.0, freq * 1.6)
    filtered_hi = _fx_filter(raw, cutoff=cutoff_hi, resonance=0.72, mode="lowpass", sr=sr)
    filtered_lo = _fx_filter(raw, cutoff=cutoff_lo, resonance=0.55, mode="lowpass", sr=sr)
    f_env = np.exp(-t / max(0.04, decay * 0.75))
    blended = filtered_lo + (filtered_hi - filtered_lo) * f_env

    env = _adsr_envelope(total_samples, note_samples, attack, decay, sustain, release, sr)
    return (np.tanh(blended * 1.8) * env * vel * 0.9).astype(np.float32)


def _synth_pluck(
    freq: float,
    dur_sec: float,
    decay: float,
    release: float,
    vel: float,
    sr: int,
) -> np.ndarray:
    total_dur = max(0.08, dur_sec + release)
    n = int(round(total_dur * sr))
    t = np.arange(n, dtype=np.float32) / sr

    # Rich harmonic pluck with faster decay on upper partials (like a plucked string)
    sig = np.zeros(n, dtype=np.float32)
    for h in range(1, 9):
        h_freq = freq * h
        if h_freq >= sr * 0.45:
            break
        h_decay = max(0.025, (decay * 1.2) / (h ** 0.75))
        sig += (1.0 / (h ** 0.85)) * np.sin(2.0 * np.pi * h_freq * t) * np.exp(-t / h_decay)

    attack_env = 1.0 - np.exp(-t * 600.0)
    return (sig * attack_env * vel * 0.55).astype(np.float32)


def _synth_fm_bell(
    freq: float,
    dur_sec: float,
    attack: float,
    decay: float,
    sustain: float,
    release: float,
    vel: float,
    sr: int,
    bright: bool = True,
) -> np.ndarray:
    note_samples = max(1, int(round(dur_sec * sr)))
    total_samples = note_samples + int(round(release * sr))
    t = np.arange(total_samples, dtype=np.float32) / sr

    ratio = 3.5 if bright else 2.0
    mod_index = (3.2 if bright else 1.8) * np.exp(-t / max(0.05, decay * 1.1))
    mod = np.sin(2.0 * np.pi * (freq * ratio) * t) * mod_index
    carrier = np.sin(2.0 * np.pi * freq * t + mod)
    sub_tone = np.sin(2.0 * np.pi * freq * t) * 0.35

    env = _adsr_envelope(total_samples, note_samples, attack, decay, sustain, release, sr)
    return ((carrier * 0.7 + sub_tone) * env * vel * 0.8).astype(np.float32)


def _synth_epiano(
    freq: float,
    dur_sec: float,
    attack: float,
    decay: float,
    sustain: float,
    release: float,
    vel: float,
    sr: int,
) -> np.ndarray:
    note_samples = max(1, int(round(dur_sec * sr)))
    total_samples = note_samples + int(round(release * sr))
    t = np.arange(total_samples, dtype=np.float32) / sr

    # Rhodes-like tine + warm fundamental FM
    tine_env = np.exp(-t * 14.0) * (1.2 * vel)
    mod1 = np.sin(2.0 * np.pi * (freq * 1.0) * t) * (0.9 * np.exp(-t * 3.5))
    mod_tine = np.sin(2.0 * np.pi * (freq * 7.0) * t) * tine_env * 0.35
    sig = np.sin(2.0 * np.pi * freq * t + mod1 + mod_tine)
    # Gentle tremolo
    trem = 1.0 - 0.12 * (0.5 + 0.5 * np.sin(2.0 * np.pi * 4.5 * t))

    env = _adsr_envelope(total_samples, note_samples, attack, decay, sustain, release, sr)
    return (sig * trem * env * vel * 0.75).astype(np.float32)


def _synth_osc(
    inst: str,
    freq: float,
    slide_freq: float | None,
    dur_sec: float,
    attack: float,
    decay: float,
    sustain: float,
    release: float,
    vel: float,
    sr: int,
) -> np.ndarray:
    note_samples = max(1, int(round(dur_sec * sr)))
    total_samples = note_samples + int(round(release * sr))
    t = np.arange(total_samples, dtype=np.float32) / sr

    if slide_freq is not None:
        glide = np.clip(t / max(0.03, dur_sec * 0.7), 0.0, 1.0)
        freq_curve = freq + (slide_freq - freq) * glide
        phase = (np.cumsum(freq_curve) / sr) % 1.0
    else:
        phase = (freq * t) % 1.0

    if inst == "sine":
        sig = np.sin(2.0 * np.pi * phase)
    elif inst == "triangle":
        sig = 2.0 * np.abs(2.0 * phase - 1.0) - 1.0
    elif inst in ("square", "pulse"):
        duty = 0.32 if inst == "pulse" else 0.5
        sig = np.where(phase < duty, 1.0, -1.0).astype(np.float32)
    elif inst in ("supersaw", "pad"):
        detunes = [0.988, 0.995, 1.0, 1.005, 1.012]
        sig = np.zeros(total_samples, dtype=np.float32)
        for d in detunes:
            p_d = ((freq * d) * t) % 1.0
            sig += 2.0 * p_d - 1.0
        sig /= len(detunes)
        if inst == "pad":
            sig = _fx_filter(sig, cutoff=min(3200.0, freq * 4.5), resonance=0.2, mode="lowpass", sr=sr)
    else:
        # Default rich detuned saw lead
        p1 = phase
        p2 = ((freq * 1.004) * t) % 1.0
        sig = ((2.0 * p1 - 1.0) + (2.0 * p2 - 1.0)) * 0.5

    env = _adsr_envelope(total_samples, note_samples, attack, decay, sustain, release, sr)
    return (sig * env * vel * 0.72).astype(np.float32)


def _fx_filter(
    buf: np.ndarray,
    cutoff: float,
    resonance: float,
    mode: str,
    sr: int,
) -> np.ndarray:
    """Fast frequency-domain resonant filter (zero-phase, vectorized via rfft)."""
    if buf.size < 8:
        return buf
    freqs = np.fft.rfftfreq(buf.size, d=1.0 / sr)
    fc = max(25.0, min(sr * 0.45, float(cutoff)))
    ratio = np.maximum(freqs, 1.0) / fc

    if mode in ("highpass", "hp"):
        resp = 1.0 / np.sqrt(1.0 + (1.0 / ratio) ** 4)
    elif mode in ("bandpass", "bp"):
        bw = max(0.25, 1.2 - resonance * 0.9)
        resp = 1.0 / np.sqrt(1.0 + ((ratio - 1.0 / ratio) / bw) ** 2)
    else:
        resp = 1.0 / np.sqrt(1.0 + ratio ** 4)

    if resonance > 0.05 and mode not in ("bandpass", "bp"):
        peak_boost = 1.0 + (resonance * 3.2) * np.exp(-((np.log2(ratio)) ** 2) / 0.08)
        resp = resp * peak_boost

    spec = np.fft.rfft(buf)
    filtered = np.fft.irfft(spec * resp.astype(np.float32), n=buf.size)
    return filtered.astype(np.float32)


def _fx_reverb(
    buf: np.ndarray,
    room: float,
    damp: float,
    mix: float,
    sr: int,
) -> np.ndarray:
    """Multi-tap diffuse Schroeder-inspired reverb."""
    if buf.size < 64:
        return buf
    delays_ms = [23.7, 31.3, 41.9, 53.3, 67.1, 83.9]
    wet = np.zeros_like(buf)
    for idx, d_ms in enumerate(delays_ms):
        shift = max(1, int(round((d_ms * (0.6 + 0.7 * room) / 1000.0) * sr)))
        gain = (0.55 * room) ** (1 + idx * 0.35)
        sign = -1.0 if idx % 2 else 1.0
        wet += np.roll(buf, shift) * (gain * sign)
    if damp > 0.05:
        wet = _fx_filter(wet, cutoff=max(900.0, 7500.0 * (1.0 - damp)), resonance=0.1, mode="lowpass", sr=sr)
    return ((1.0 - mix) * buf + mix * wet).astype(np.float32)


# ── Multi-Track Mixing & Export Utilities (WAV / Stems / MIDI) ──────────


def mix_tracks_stereo(
    tracks: list[Track],
    sample_rate: int = SAMPLE_RATE,
    master_volume: float = 0.9,
) -> np.ndarray:
    """Mix a list of Tracks into an interleaved/2D stereo float32 buffer of shape `(N, 2)`."""
    if not tracks:
        return np.zeros((sample_rate, 2), dtype=np.float32)

    any_solo = any(t.solo for t in tracks)
    active = [t for t in tracks if (t.solo if any_solo else not t.muted)]
    if not active:
        active = tracks

    max_dur = max(t.duration for t in active)
    n_samples = max(1, int(round(max_dur * sample_rate)))
    left = np.zeros(n_samples, dtype=np.float32)
    right = np.zeros(n_samples, dtype=np.float32)

    for tr in active:
        mono = tr.render(sample_rate=sample_rate, apply_volume=True)
        if mono.size == 0:
            continue
        # Tile if shorter than max_dur (e.g. a 1-bar drum loop under a 2-bar melody)
        if mono.size < n_samples:
            reps = math.ceil(n_samples / mono.size)
            mono = np.tile(mono, reps)[:n_samples]
        else:
            mono = mono[:n_samples]

        # Constant-power stereo pan
        angle = (tr.pan + 1.0) * (math.pi / 4.0)
        l_gain = math.cos(angle)
        r_gain = math.sin(angle)
        left += mono * l_gain
        right += mono * r_gain

    stereo = np.column_stack([left, right]) * float(master_volume)
    peak = float(np.max(np.abs(stereo))) if stereo.size else 0.0
    if peak > 0.98:
        stereo = np.tanh(stereo * 1.05) * 0.96
    return stereo.astype(np.float32)


def write_wav_file(path: str, audio: np.ndarray, sample_rate: int = SAMPLE_RATE) -> None:
    """Write a mono `(N,)` or stereo `(N, 2)` float32 array to a 16-bit PCM `.wav` file."""
    clipped = np.clip(audio, -1.0, 1.0)
    pcm16 = (clipped * 32767.0).astype("<i2")
    n_channels = 2 if (pcm16.ndim == 2 and pcm16.shape[1] == 2) else 1
    with wave.open(path, "wb") as wf:
        wf.setnchannels(n_channels)
        wf.setsampwidth(2)
        wf.setframerate(sample_rate)
        wf.writeframes(pcm16.tobytes())


def write_midi_file(path: str, tracks: list[Track]) -> None:
    """Write a Standard MIDI File (SMF Format 1, `.mid`) containing all tracks and their notes."""
    tpqn = 480  # Ticks per quarter note (120 ticks per 16th-note step)
    ticks_per_step = tpqn // 4

    bpm = tracks[0].bpm if tracks else _GLOBAL_BPM
    tempo_us = int(round(60_000_000.0 / max(30.0, bpm)))

    midi_chunks: list[bytes] = []

    # Track 0: Tempo & Time Signature meta track
    t0 = bytearray()
    t0 += _vlq(0) + b"\xFF\x51\x03" + tempo_us.to_bytes(3, "big")
    t0 += _vlq(0) + b"\xFF\x58\x04\x04\x02\x18\x08"
    t0 += _vlq(0) + b"\xFF\x2F\x00"
    midi_chunks.append(b"MTrk" + struct.pack(">I", len(t0)) + bytes(t0))

    for idx, tr in enumerate(tracks):
        is_drum = tr.instrument in (
            "kick", "snare", "clap", "hihat", "closed_hat", "open_hat", "perc", "rim", "tom", "cowbell"
        )
        channel = 9 if is_drum else (idx % 15 if idx % 15 < 9 else (idx % 15) + 1)

        raw_events: list[tuple[int, int, bytes]] = []
        for ev in tr.events:
            on_tick = max(0, int(round(ev.step * ticks_per_step)))
            dur_ticks = max(15, int(round(ev.duration * ticks_per_step)))
            off_tick = on_tick + dur_ticks
            note_num = max(0, min(127, int(ev.midi)))
            vel_num = max(1, min(127, int(round(ev.velocity * 100.0))))
            raw_events.append((on_tick, 1, bytes([0x90 | channel, note_num, vel_num])))
            raw_events.append((off_tick, 0, bytes([0x80 | channel, note_num, 0])))

        raw_events.sort(key=lambda item: (item[0], item[1]))

        trk = bytearray()
        name_bytes = tr.name.encode("utf-8")[:64]
        trk += _vlq(0) + b"\xFF\x03" + _vlq(len(name_bytes)) + name_bytes

        last_tick = 0
        for tick, _, msg in raw_events:
            delta = max(0, tick - last_tick)
            trk += _vlq(delta) + msg
            last_tick = tick

        trk += _vlq(0) + b"\xFF\x2F\x00"
        midi_chunks.append(b"MTrk" + struct.pack(">I", len(trk)) + bytes(trk))

    header = b"MThd" + struct.pack(">IHHH", 6, 1, len(midi_chunks), tpqn)
    with open(path, "wb") as f:
        f.write(header)
        for ch in midi_chunks:
            f.write(ch)


def _vlq(val: int) -> bytes:
    """Encode an integer as a MIDI variable-length quantity."""
    v = max(0, int(val))
    out = [v & 0x7F]
    v >>= 7
    while v > 0:
        out.append((v & 0x7F) | 0x80)
        v >>= 7
    out.reverse()
    return bytes(out)
