"""Audio & musical measurements on the live MusicScript Tracks and Notes in the scene registry."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Mapping

from .scene import Registry
from .synth import NoteEvent, Track, midi_to_freq, midi_to_note

MAX_SELECTIONS = 64


class MeasurementError(ValueError):
    """Invalid or stale selection supplied by the GUI."""


@dataclass(frozen=True)
class TopologyReference:
    key: str
    part: int
    kind: str
    identifier: int

    @classmethod
    def from_message(cls, value: Any) -> TopologyReference:
        if not isinstance(value, Mapping):
            raise MeasurementError("Each measurement selection must be an object")
        key = value.get("key")
        kind = value.get("kind")
        part = value.get("part")
        identifier = value.get("identifier")
        if not isinstance(key, str) or not key:
            raise MeasurementError("A measurement selection needs an object key")
        if kind not in {"face", "edge", "vertex"}:
            raise MeasurementError(f"Unknown topology kind {kind!r}")
        if isinstance(part, bool) or not isinstance(part, int) or part < 0:
            raise MeasurementError("Topology part must be a non-negative integer")
        if (
            isinstance(identifier, bool)
            or not isinstance(identifier, int)
            or identifier < 1
        ):
            raise MeasurementError("Topology identifier must be a positive integer")
        return cls(key, part, kind, identifier)


def measure(
    registry: Registry, raw_selections: Any, raw_objects: Any = None
) -> dict[str, Any]:
    """Describe each selected note/step or selected Track."""
    references, items = resolve(registry, raw_selections, raw_objects)

    result: dict[str, Any] = {
        "identity": {"scope": "scene_entry", "persistent": False},
        "entities": (
            [
                _describe_entity(reference, item)
                for reference, item in zip(references, items, strict=True)
            ]
            if references
            else []
        ),
        "objects": (
            []
            if references
            else [
                _describe_object(key, item)
                for key, item in zip(_object_keys(raw_objects), items, strict=True)
            ]
        ),
        "pair": None,
        "bounding_box": None,
    }
    if len(references) == 2:
        result["pair"] = _describe_pair(items[0], items[1])
    return result


def resolve(
    registry: Registry, raw_selections: Any, raw_objects: Any = None
) -> tuple[list[TopologyReference], list[Any]]:
    raw_selections = [] if raw_selections is None else raw_selections
    if not isinstance(raw_selections, list) or len(raw_selections) > MAX_SELECTIONS:
        raise MeasurementError(
            f"Select between one and {MAX_SELECTIONS} notes or tracks to analyze"
        )
    if not raw_selections:
        keys = _object_keys(raw_objects)
        if not keys:
            raise MeasurementError(
                f"Select between one and {MAX_SELECTIONS} notes or tracks to analyze"
            )
        return [], [object_shape(registry, key) for key in keys]

    references = [TopologyReference.from_message(value) for value in raw_selections]
    resolved: list[tuple[Track, NoteEvent]] = []
    for reference in references:
        try:
            item = registry.get(reference.key).subshape(
                reference.part, reference.kind, reference.identifier
            )
        except (IndexError, KeyError, ValueError) as error:
            raise MeasurementError(str(error)) from None
        resolved.append(item)
    return references, resolved


def _object_keys(raw_objects: Any) -> list[str]:
    if raw_objects is None:
        return []
    if (
        not isinstance(raw_objects, list)
        or len(raw_objects) > MAX_SELECTIONS
        or not all(isinstance(key, str) and key for key in raw_objects)
    ):
        raise MeasurementError(
            f"Tracks to measure must be at most {MAX_SELECTIONS} scene keys"
        )
    return raw_objects


def object_shape(registry: Registry, key: str) -> Track:
    try:
        tracks = registry.subtree_shapes([key])
    except KeyError as error:
        raise MeasurementError(error.args[0]) from None
    if not tracks:
        raise MeasurementError(f"Track {key!r} has no audio data to analyze")
    return tracks[0]


def _describe_object(key: str, track: Track) -> dict[str, Any]:
    m = track.metrics()
    return {
        "key": key,
        "shape_type": f"{track.instrument.upper()} ({track.bpm:g} BPM)",
        "volume": float(m["duration"]),
        "area": float(m["rms_db"]),
        "length": float(m["peak_db"]),
        "center": [float(m["sub_energy"]), float(m["bass_energy"]), float(m["mid_energy"])],
        "bounding_box": {
            "min": [0.0, 0.0, 0.0],
            "max": [float(track.total_steps), float(track.bars), float(m["crest_db"])],
            "size": [float(track.total_steps), float(track.bars), float(m["crest_db"])],
            "center": [float(m["sub_energy"]), float(m["bass_energy"]), float(m["high_energy"])],
        },
        "solids": int(track.bars),
        "faces": len(track.events),
        "edges": int(track.total_steps),
        "vertices": len(track.effects),
    }


def _describe_entity(
    reference: TopologyReference, item: tuple[Track, NoteEvent]
) -> dict[str, Any]:
    track, ev = item
    note_str = midi_to_note(ev.midi)
    freq = midi_to_freq(ev.midi)
    center = [float(ev.step), float(ev.midi), float(ev.velocity)]
    return {
        "key": reference.key,
        "part": reference.part,
        "kind": reference.kind,
        "identifier": reference.identifier,
        "geometry_type": f"note {note_str} ({freq:.1f} Hz)",
        "center": center,
        "position": center,
        "length": float(ev.duration),
        "area": float(ev.velocity * 100.0),
        "radius": float(freq),
        "diameter": float(ev.midi),
        "bounding_box": None,
    }


def _describe_pair(
    first: tuple[Track, NoteEvent], second: tuple[Track, NoteEvent]
) -> dict[str, Any]:
    _t1, ev1 = first
    _t2, ev2 = second
    step_dist = abs(float(ev2.step) - float(ev1.step))
    semitones = abs(int(ev2.midi) - int(ev1.midi))
    return {
        "distance": step_dist,
        "first_point": [float(ev1.step), float(ev1.midi), float(ev1.velocity)],
        "second_point": [float(ev2.step), float(ev2.midi), float(ev2.velocity)],
        "angle_degrees": float(semitones),
        "angle_kind": "semitones",
    }
