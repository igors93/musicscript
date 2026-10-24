"""What is picked in the GUI, as MusicScript code: note/pattern expressions and live tracks."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .measurement import MeasurementError, TopologyReference, object_shape
from .scene import Registry
from .synth import midi_to_freq, midi_to_note


class SelectorError(MeasurementError):
    """The picked entities cannot be described by a selector."""


@dataclass(frozen=True)
class PickedSelection:
    """What the GUI had selected when it sent a console command."""

    entities: tuple[TopologyReference, ...] = ()
    objects: tuple[str, ...] = ()

    @classmethod
    def from_message(cls, message: Any) -> PickedSelection:
        if message is None:
            return cls()
        if not isinstance(message, dict):
            raise MeasurementError("The selection must be an object")
        entities = message.get("entities", [])
        objects = message.get("objects", [])
        if not isinstance(entities, list) or not isinstance(objects, list):
            raise MeasurementError("Selected entities and objects must be lists")
        if not all(isinstance(key, str) and key for key in objects):
            raise MeasurementError("Selected objects must be scene keys")
        return cls(
            tuple(TopologyReference.from_message(value) for value in entities),
            tuple(objects),
        )

    def resolve(self, registry: Registry) -> list[Any]:
        try:
            if self.entities:
                return [
                    registry.get(r.key).subshape(r.part, r.kind, r.identifier)[1]
                    for r in self.entities
                ]
            return [object_shape(registry, key) for key in self.objects]
        except KeyError as error:
            raise LookupError(error.args[0]) from None
        except MeasurementError as error:
            raise LookupError(str(error)) from None
        except (IndexError, ValueError) as error:
            raise LookupError(
                f"{error}; the beat changed since it was picked, pick it again"
            ) from None


def selector_candidates(registry: Registry, raw_selections: Any) -> dict[str, Any]:
    """Generate MusicScript note/pattern method calls for picked note blocks."""
    if not isinstance(raw_selections, list) or not raw_selections:
        raise SelectorError("Select note blocks in the view to get a MusicScript snippet")
    references = [TopologyReference.from_message(value) for value in raw_selections]
    first = references[0]
    try:
        entry = registry.get(first.key)
    except KeyError as error:
        raise SelectorError(error.args[0]) from None

    track, ev = entry.subshape(first.part, first.kind, first.identifier)
    note_str = midi_to_note(ev.midi)
    freq = midi_to_freq(ev.midi)

    candidates = [
        {
            "expression": f".hit(step={ev.step:g}, note={note_str!r}, duration={ev.duration:g}, velocity={ev.velocity:.2f})",
            "selector": note_str,
            "description": f"Trigger {note_str} ({freq:.1f} Hz) at step {ev.step:g} on '{track.name}'",
            "index_based": False,
        }
    ]
    if track._pattern_str:
        candidates.append(
            {
                "expression": f".pattern({track._pattern_str!r})",
                "selector": track._pattern_str,
                "description": f"Full step pattern of '{track.name}'",
                "index_based": False,
            }
        )
    elif track._notes_str:
        candidates.append(
            {
                "expression": f".notes({track._notes_str!r})",
                "selector": track._notes_str,
                "description": f"Full note sequence of '{track.name}'",
                "index_based": False,
            }
        )
    return {
        "method": "hit",
        "candidates": candidates,
    }
