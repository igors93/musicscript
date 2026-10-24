"""The interactive Python console behind the GUI's console panel."""

from __future__ import annotations

import builtins
import code
from collections.abc import Callable
import sys
from typing import Any

from . import synth
from .display import Displayer
from .execution import ShownObjects, rand_color, script_log


class ConsoleSession:
    """A persistent MusicScript interpreter."""

    def __init__(self, displayer: Displayer | None = None) -> None:
        self.shown = ShownObjects()
        self.namespace: dict[str, Any] = {}
        self._selection: Callable[[], list[Any]] = list
        self._displayer = displayer
        self.reset()

    def reset(self) -> None:
        def selected() -> list[Any]:
            """The notes or tracks currently selected in the Studio View."""
            return list(self._selection())

        self.namespace.clear()
        self.namespace.update(
            {
                "__name__": "__console__",
                "__builtins__": builtins,
                "ms": synth,
                "musicscript": synth,
                "cq": synth,
                "Beat": synth.Beat,
                "Track": synth.Track,
                "kick": synth.kick,
                "snare": synth.snare,
                "clap": synth.clap,
                "hihat": synth.hihat,
                "open_hat": synth.open_hat,
                "perc": synth.perc,
                "bass808": synth.bass808,
                "sub": synth.sub,
                "synth": synth.synth,
                "pluck": synth.pluck,
                "pad": synth.pad,
                "keys": synth.keys,
                "acid": synth.acid,
                "bell": synth.bell,
                "set_bpm": synth.set_bpm,
                "get_bpm": synth.get_bpm,
                "set_swing": synth.set_swing,
                "set_bars": synth.set_bars,
                "note_to_midi": synth.note_to_midi,
                "midi_to_note": synth.midi_to_note,
                "note_to_freq": synth.note_to_freq,
                "chord_notes": synth.chord_notes,
                "scale_notes": synth.scale_notes,
                "show_object": self._show_object,
                "show_track": self._show_object,
                "add_track": self._show_object,
                "show_beat": self._show_object,
                "show": self._show_object,
                "debug": self._debug,
                "rand_color": rand_color,
                "log": lambda value: script_log.info(str(value)),
                "selected": selected,
                "display": self._display,
            }
        )
        self._interpreter = code.InteractiveConsole(
            self.namespace, filename="<console>"
        )

    def set_selection(self, resolve: Callable[[], list[Any]]) -> None:
        self._selection = resolve

    def update_variables(self, variables: dict[str, Any]) -> None:
        self.namespace.update(
            (name, value)
            for name, value in variables.items()
            if not name.startswith("__")
        )

    def execute(self, source: str, request_id: int | None = None) -> bool:
        more = False
        previous = sys.displayhook
        if self._displayer is not None:
            self._displayer.start(request_id)
            sys.displayhook = self._displayer.displayhook
        try:
            for line in source.split("\n"):
                more = self._interpreter.push(line)
        finally:
            sys.displayhook = previous
            if self._displayer is not None:
                self._displayer.finish()
        return more

    def take_shown(self) -> ShownObjects:
        shown, self.shown = self.shown, ShownObjects()
        return shown

    def _show_object(self, obj, name=None, options=None, **kwargs) -> None:
        self.shown.show(obj, name, options, **kwargs)

    def _debug(self, obj, name=None) -> None:
        self.shown.debug(obj, name)

    def _display(self, *values) -> None:
        if self._displayer is None:
            print(*values)
        else:
            self._displayer.display(*values)
