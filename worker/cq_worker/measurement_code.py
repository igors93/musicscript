"""Generate ready-to-paste MusicScript code from the Audio Analyzer / Measurements panel."""

from __future__ import annotations

from typing import Any

from . import measurement
from .scene import Registry
from .synth import Track, midi_to_freq, midi_to_note


def measurement_code(
    registry: Registry,
    namespace: dict[str, Any],
    raw_selections: Any,
    raw_objects: Any = None,
) -> dict[str, Any]:
    """Generate MusicScript code reproducing or inspecting the selected track/notes."""
    del namespace
    references, items = measurement.resolve(registry, raw_selections, raw_objects)
    lines: list[str] = []

    if references:
        for idx, (ref, (track, ev)) in enumerate(zip(references, items, strict=True), start=1):
            del ref
            var_name = track.name.lower().replace(" ", "_").replace("-", "_")
            note_str = midi_to_note(ev.midi)
            freq = midi_to_freq(ev.midi)
            lines.append(
                f"# Note #{idx} on '{track.name}': {note_str} ({freq:.1f} Hz) at step {ev.step:g}"
            )
            lines.append(
                f"{var_name}.hit(step={ev.step:g}, note={note_str!r}, duration={ev.duration:g}, velocity={ev.velocity:.2f})"
            )
    else:
        for track in items:
            if not isinstance(track, Track):
                continue
            m = track.metrics()
            var_name = track.name.lower().replace(" ", "_").replace("-", "_")
            lines.append(
                f"# Track '{track.name}' [{track.instrument}] — Peak {m['peak_db']:.1f} dBFS | RMS {m['rms_db']:.1f} dBFS | {track.bpm:g} BPM"
            )
            if track._pattern_str:
                lines.append(
                    f"{var_name} = Track({track.name!r}, instrument={track.instrument!r}, volume={track.volume:.2f}, pan={track.pan:.2f}).pattern({track._pattern_str!r})"
                )
            elif track._notes_str:
                lines.append(
                    f"{var_name} = Track({track.name!r}, instrument={track.instrument!r}, volume={track.volume:.2f}, pan={track.pan:.2f}).notes({track._notes_str!r})"
                )
            else:
                lines.append(
                    f"{var_name} = Track({track.name!r}, instrument={track.instrument!r}, volume={track.volume:.2f}, pan={track.pan:.2f})"
                )

    text = "\n".join(lines) + "\n"
    return {"code": text, "verified": True, "problem": None}
