"""Compile and run MusicScript beat scripts in the worker subprocess."""

from __future__ import annotations

from collections.abc import Callable, Iterator
import contextlib
from dataclasses import dataclass
import importlib
from inspect import currentframe
import linecache
import logging
import os
import random
import sys
import sysconfig
import traceback
import types
from typing import Any

from . import provenance, synth
from .debugger import DebugStop
from .scene import ShownObject

# Register `musicscript` and `ms` as importable module aliases for user scripts & Jedi
sys.modules.setdefault("musicscript", synth)
sys.modules.setdefault("ms", synth)
sys.modules.setdefault("cadquery", synth)

#: Pseudo file name of the editor buffer in code objects and tracebacks.
SCRIPT_FILENAME = "<cq-editor>"

#: Seed used before every run, so that ``rand_color`` is reproducible.
RANDOM_SEED = 59798267586177

#: Longest value text sent for the variables panel.
MAX_VALUE_LENGTH = 2000

script_log = logging.getLogger("script")


@dataclass
class RunSettings:
    reload_cq: bool = False
    add_script_dir_to_path: bool = True
    change_working_dir: bool = True
    reload_imported_modules: bool = True

    @classmethod
    def from_message(cls, message: dict | None) -> RunSettings:
        message = message or {}
        return cls(
            **{
                name: bool(message[name])
                for name in cls.__dataclass_fields__
                if name in message
            }
        )


def rand_color(alpha: float = 1.0, cfloat: bool = False):
    """Random vibrant studio color for ``show_object`` / ``show_track`` options."""
    lower, upper = 50, 240
    channels = [random.randrange(lower, upper) for _ in range(3)]
    if cfloat:
        return (*(c / 255 for c in channels), alpha)
    return {"alpha": alpha, "color": tuple(channels)}


class ShownObjects(dict[str, ShownObject]):
    """Objects passed to ``show_object`` / ``show_track``, in call order, keyed by name."""

    def show(self, obj: Any, name: str | None = None, options=None, **kwargs) -> None:
        merged = {**(options or {}), **kwargs}
        caller = currentframe().f_back.f_back  # type: ignore[union-attr]
        line = _script_line(caller)
        if isinstance(obj, synth.Beat):
            for tr in obj.tracks:
                tname = self._unique_custom_name(tr.name or "Track")
                self[tname] = ShownObject(tr, merged, provenance.creation_line(tr) or line)
            return
        if isinstance(obj, (list, tuple)) and all(isinstance(x, synth.Track) for x in obj):
            for tr in obj:
                tname = self._unique_custom_name(tr.name or "Track")
                self[tname] = ShownObject(tr, merged, provenance.creation_line(tr) or line)
            return

        if not name:
            name = getattr(obj, "name", None) or _variable_name(obj, caller) or self._unique_name(obj)
        self[self._unique_custom_name(str(name))] = ShownObject(obj, merged, line)

    def debug(self, obj: Any, name: str | None = None) -> None:
        caller = currentframe().f_back.f_back  # type: ignore[union-attr]
        if not name:
            name = getattr(obj, "name", None) or _variable_name(obj, caller) or self._unique_name(obj)
        self[self._unique_custom_name(str(name))] = ShownObject(
            obj, {"color": "red", "alpha": 0.45}, _script_line(caller)
        )

    def _unique_name(self, obj: Any) -> str:
        base = getattr(obj, "name", None) or type(obj).__name__
        return self._unique_custom_name(str(base))

    def _unique_custom_name(self, base: str) -> str:
        if base not in self:
            return base
        index = 2
        while f"{base} #{index}" in self:
            index += 1
        return f"{base} #{index}"


def _script_line(frame) -> int | None:
    if frame is not None and frame.f_code.co_filename == SCRIPT_FILENAME:
        return frame.f_lineno
    return None


def _variable_name(obj: Any, frame) -> str | None:
    if frame is None:
        return None
    for name, value in frame.f_locals.items():
        if value is obj and not name.startswith("_"):
            return name
    return None


def script_api(shown: ShownObjects) -> dict[str, Any]:
    """Names injected into the MusicScript namespace."""

    def show_object(obj, name=None, options=None, **kwargs):
        shown.show(obj, name, options, **kwargs)
        return obj

    def debug(obj, name=None):
        shown.debug(obj, name)
        return obj

    return {
        "show_object": show_object,
        "show_track": show_object,
        "add_track": show_object,
        "show_beat": show_object,
        "debug": debug,
        "rand_color": rand_color,
        "log": lambda value: script_log.info(str(value)),
        # MusicScript core API
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
    }


def compile_script(source: str) -> types.CodeType:
    lines = source.splitlines(keepends=True)
    linecache.cache[SCRIPT_FILENAME] = (len(source), None, lines, SCRIPT_FILENAME)
    return compile(source, SCRIPT_FILENAME, "exec")


def new_module(path: str | None) -> types.ModuleType:
    module = types.ModuleType("__cq_main__")
    if path:
        module.__file__ = path
    return module


@contextlib.contextmanager
def script_environment(path: str | None, settings: RunSettings) -> Iterator[None]:
    script_dir = os.path.dirname(os.path.abspath(path)) if path else None
    with contextlib.ExitStack() as stack:
        if script_dir and os.path.isdir(script_dir):
            if settings.add_script_dir_to_path:
                sys.path.insert(0, script_dir)
                stack.callback(_remove_path, script_dir)
            if settings.change_working_dir:
                stack.enter_context(working_directory(script_dir))
        if settings.reload_imported_modules:
            stack.enter_context(unload_new_user_modules())
        yield


@contextlib.contextmanager
def working_directory(path: str) -> Iterator[None]:
    previous = os.getcwd()
    os.chdir(path)
    try:
        yield
    finally:
        os.chdir(previous)


def _remove_path(entry: str) -> None:
    with contextlib.suppress(ValueError):
        sys.path.remove(entry)


@contextlib.contextmanager
def unload_new_user_modules() -> Iterator[None]:
    before = set(sys.modules)
    try:
        yield
    finally:
        for name in set(sys.modules) - before:
            module = sys.modules.get(name)
            if module is not None and not _is_library_module(module):
                del sys.modules[name]


_LIBRARY_ROOTS = tuple(
    os.path.normcase(os.path.abspath(p))
    for p in {
        sys.prefix,
        sys.base_prefix,
        sys.exec_prefix,
        *(
            sysconfig.get_paths().get(k, "")
            for k in ("stdlib", "platstdlib", "purelib", "platlib")
        ),
    }
    if p
)


def _is_library_module(module: types.ModuleType) -> bool:
    path = getattr(module, "__file__", None)
    if not path:
        return True
    path = os.path.normcase(os.path.abspath(path))
    return path.startswith(_LIBRARY_ROOTS)


def reload_cadquery() -> None:
    importlib.reload(synth)


def describe_variables(namespace: dict[str, Any], hidden: set[str]) -> list[dict]:
    rows = []
    for name, value in namespace.items():
        if name.startswith("_") or name in hidden:
            continue
        try:
            if isinstance(value, synth.Track):
                text = f"Track({value.name!r}, instrument={value.instrument!r}, {len(value.events)} notes, {value.bpm:g} BPM)"
            elif isinstance(value, synth.Beat):
                text = f"Beat({value.name!r}, {len(value.tracks)} tracks, {value.bpm:g} BPM, {value.bars} bars)"
            else:
                text = str(value)
        except Exception as error:
            text = f"<unprintable: {type(error).__name__}>"
        if len(text) > MAX_VALUE_LENGTH:
            text = text[: MAX_VALUE_LENGTH - 1] + "…"
        rows.append({"name": name, "type": type(value).__name__, "value": text})
    return rows


def describe_exception(error: BaseException) -> dict:
    frames = traceback.extract_tb(error.__traceback__)
    while frames and frames[0].filename != SCRIPT_FILENAME:
        frames.pop(0)

    rows = [
        {
            "file": frame.filename,
            "line": frame.lineno,
            "function": frame.name,
            "code": (frame.line or "").strip(),
        }
        for frame in frames
    ]
    if isinstance(error, SyntaxError):
        rows.append(
            {
                "file": error.filename or SCRIPT_FILENAME,
                "line": error.lineno or 0,
                "function": "",
                "code": (error.text or "").strip(),
            }
        )

    script_lines = [row["line"] for row in rows if row["file"] == SCRIPT_FILENAME]
    return {
        "type_name": type(error).__name__,
        "message": str(error),
        "frames": rows,
        "line": script_lines[-1] if script_lines else None,
        "text": "".join(traceback.format_exception(error)),
    }


class Execution:
    """One run of the editor buffer."""

    def __init__(self, source: str, path: str | None, settings: RunSettings) -> None:
        self.source = source
        self.path = path
        self.settings = settings
        self.module = new_module(path)
        self.shown = ShownObjects()
        self.api = script_api(self.shown)
        self.error: dict | None = None
        self.stopped = False
        self.interrupted = False

    @property
    def namespace(self) -> dict[str, Any]:
        return self.module.__dict__

    def run(
        self, executor: Callable[[types.CodeType, dict], None] | None = None
    ) -> bool:
        random.seed(RANDOM_SEED)
        importlib.invalidate_caches()
        if self.settings.reload_cq:
            reload_cadquery()

        synth.reset_session()

        try:
            code = compile_script(self.source)
        except SyntaxError as error:
            self.error = describe_exception(error)
            return False

        self.namespace.update(self.api)
        try:
            with script_environment(self.path, self.settings):
                with provenance.recording(SCRIPT_FILENAME):
                    (executor or _exec)(code, self.namespace)
        except DebugStop:
            self.stopped = True
        except SystemExit:
            pass
        except KeyboardInterrupt as error:
            self.interrupted = True
            self.error = describe_exception(error)
            return False
        except Exception as error:
            sys.last_traceback = error.__traceback__
            self.error = describe_exception(error)
            return False
        finally:
            self._remove_api()
        return True

    def hidden_names(self) -> set[str]:
        return set(self.api) - {"ms", "musicscript", "cq"}

    def _remove_api(self) -> None:
        for name, value in self.api.items():
            if name not in ("ms", "musicscript", "cq") and self.namespace.get(name) is value:
                del self.namespace[name]


def _exec(code: types.CodeType, namespace: dict) -> None:
    exec(code, namespace)
