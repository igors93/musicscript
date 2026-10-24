"""The line of the script that created each MusicScript Track or Beat, for the GUI's *Go to source*."""

from __future__ import annotations

from collections.abc import Iterator
import contextlib
import functools
import inspect
import sys
from typing import Any

from . import synth

#: Attribute holding the 1-based script line on recorded objects.
LINE_ATTRIBUTE = "_cq_editor_line"


def creation_line(obj: Any) -> int | None:
    """The script line that created ``obj``, if it was recorded."""
    line = getattr(obj, "__dict__", {}).get(LINE_ATTRIBUTE)
    return line if isinstance(line, int) else None


def _recorded_classes() -> list[type]:
    return [synth.Track, synth.Beat]


@contextlib.contextmanager
def recording(filename: str) -> Iterator[None]:
    """Records creation lines in the code of ``filename`` while active."""

    def script_line() -> int | None:
        try:
            frame = sys._getframe(2)
        except ValueError:
            return None
        while frame is not None:
            if frame.f_code.co_filename == filename:
                return frame.f_lineno
            frame = frame.f_back
        return None

    def wrapped_init(original):
        @functools.wraps(original)
        def __init__(self, *args, **kwargs):
            original(self, *args, **kwargs)
            attributes = getattr(self, "__dict__", None)
            if attributes is not None and LINE_ATTRIBUTE not in attributes:
                line = script_line()
                if line is not None:
                    attributes[LINE_ATTRIBUTE] = line

        return __init__

    def wrapped_method(original):
        @functools.wraps(original)
        def method(self, *args, **kwargs):
            prev_fx = len(getattr(self, "effects", ()))
            result = original(self, *args, **kwargs)
            line = script_line()
            if line is not None:
                effects = getattr(self, "effects", None)
                if isinstance(effects, list) and len(effects) > prev_fx:
                    for fx in effects[prev_fx:]:
                        if getattr(fx, "line", None) is None:
                            fx.line = line
                if result is not None and result is not self:
                    attrs = getattr(result, "__dict__", None)
                    if attrs is not None and LINE_ATTRIBUTE not in attrs:
                        attrs[LINE_ATTRIBUTE] = line
            return result

        return method

    patched: list[tuple[type, str, Any]] = []
    try:
        for cls in _recorded_classes():
            orig_init = cls.__dict__["__init__"]
            cls.__init__ = wrapped_init(orig_init)  # type: ignore[misc]
            patched.append((cls, "__init__", orig_init))
            for name, attr in list(inspect.getmembers(cls, predicate=inspect.isfunction)):
                if name.startswith("_"):
                    continue
                setattr(cls, name, wrapped_method(attr))
                patched.append((cls, name, attr))
        yield
    finally:
        for cls, name, original in reversed(patched):
            setattr(cls, name, original)
