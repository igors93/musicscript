"""MusicScript Track & Beat signal-chain inspection."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from .provenance import creation_line
from .synth import Beat, Track, midi_to_freq, midi_to_note
from .tessellation import Tolerance, tessellate

MAX_VALUE_LENGTH = 200


@dataclass
class StageItem:
    type_name: str
    value: str
    track: Track
    line: int | None = None
    displayable: bool = True


@dataclass
class StageFrame:
    type_name: str
    value: str
    track: Track
    line: int | None = None
    items: list[StageItem] = field(default_factory=list)


def stack_frames(obj: Any) -> list[StageFrame]:
    """Build signal-chain frames for a `Track` or `Beat`."""
    if isinstance(obj, Beat):
        return [
            StageFrame(
                type_name=f"Track[{tr.instrument}]",
                value=f"{tr.name} · {len(tr.events)} notes · {tr.bpm:g} BPM",
                track=tr,
                line=creation_line(tr),
                items=[
                    StageItem(
                        type_name="NoteEvent",
                        value=f"step {ev.step:g}: {midi_to_note(ev.midi)} (vel={ev.velocity:.2f})",
                        track=tr,
                        line=creation_line(tr),
                    )
                    for ev in tr.events[:24]
                ],
            )
            for tr in obj.tracks
        ]

    if not isinstance(obj, Track):
        return []

    tr = obj
    base_line = creation_line(tr)
    frames: list[StageFrame] = []

    # 1. Sequence / Pattern stage
    seq_summary = tr._pattern_str or tr._notes_str or f"{len(tr.events)} events"
    if len(seq_summary) > 36:
        seq_summary = seq_summary[:33] + "..."
    note_items = [
        StageItem(
            type_name="NoteEvent",
            value=f"step {ev.step:g} (dur {ev.duration:g}): {midi_to_note(ev.midi)} · {midi_to_freq(ev.midi):.1f} Hz · vel {ev.velocity:.2f}",
            track=tr,
            line=base_line,
        )
        for ev in tr.events[:32]
    ]
    frames.append(
        StageFrame(
            type_name="1. Sequence",
            value=f"{seq_summary} ({tr.bars} bars, {tr.total_steps} steps @ {tr.bpm:g} BPM)",
            track=tr,
            line=base_line,
            items=note_items,
        )
    )

    # 2. Synthesizer / Drum Generator stage
    frames.append(
        StageFrame(
            type_name=f"2. Synth[{tr.instrument}]",
            value=f"ADSR({tr.attack*1000:.0f}ms, {tr.decay*1000:.0f}ms, {tr.sustain:.2f}, {tr.release*1000:.0f}ms)",
            track=tr,
            line=base_line,
            items=[
                StageItem(
                    type_name="Oscillator",
                    value=f"instrument={tr.instrument!r}, base_note={midi_to_note(tr.base_midi)}",
                    track=tr,
                    line=base_line,
                )
            ],
        )
    )

    # 3..N. DSP Effects chain stages
    for idx, fx in enumerate(tr.effects):
        params_str = ", ".join(f"{k}={v}" for k, v in fx.params.items())
        frames.append(
            StageFrame(
                type_name=f"{idx + 3}. FX:{fx.kind}",
                value=params_str,
                track=tr,
                line=fx.line or base_line,
                items=[
                    StageItem(
                        type_name=fx.kind.title(),
                        value=params_str,
                        track=tr,
                        line=fx.line or base_line,
                    )
                ],
            )
        )

    # Final Output stage
    metrics = tr.metrics()
    frames.append(
        StageFrame(
            type_name=f"{len(frames) + 1}. Output",
            value=f"Vol {tr.volume:.2f} · Pan {tr.pan:+.2f} · Peak {metrics['peak_db']:.1f} dBFS · RMS {metrics['rms_db']:.1f} dBFS",
            track=tr,
            line=base_line,
            items=[
                StageItem(
                    type_name="AudioMetrics",
                    value=f"Peak={metrics['peak_db']:.1f} dB, RMS={metrics['rms_db']:.1f} dB, Crest={metrics['crest_db']:.1f} dB",
                    track=tr,
                    line=base_line,
                )
            ],
        )
    )
    return frames


def describe_stack(frames: list[StageFrame]) -> list[dict[str, Any]]:
    return [
        {
            "type": frame.type_name,
            "value": frame.value,
            "line": frame.line,
            "items": [
                {
                    "type": item.type_name,
                    "value": item.value,
                    "displayable": item.displayable,
                    "line": item.line,
                }
                for item in frame.items
            ],
        }
        for frame in frames
    ]


def frame_geometry(
    frame: StageFrame, item: int | None, tolerance: Tolerance
) -> dict[str, Any]:
    del item
    mesh = tessellate(frame.track, tolerance=tolerance, lane_index=0)
    meshes = [mesh.to_message()] if not mesh.is_empty() else []
    plane = {
        "origin": [0.0, 0.0, 0.0],
        "x_dir": [1.0, 0.0, 0.0],
        "z_dir": [0.0, 0.0, 1.0],
        "size": float(max(20.0, frame.track.total_steps * 2.0)),
    }
    return {"meshes": meshes, "plane": plane}
