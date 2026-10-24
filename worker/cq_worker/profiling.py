"""Per-line profiling of a MusicScript run."""

from __future__ import annotations

from collections import defaultdict
import os
import sys
import time
import types
from typing import Any

from . import synth

#: MusicScript synth engine path: calls into it are profiled as operations.
_SYNTH_FILE = os.path.abspath(synth.__file__)
OPERATIONS_PER_LINE = 8


class LineProfiler:
    """Runs a script and measures per-line and per-synth-operation time."""

    def __init__(self, filename: str) -> None:
        self.filename = filename
        self._lines: defaultdict[int, list] = defaultdict(lambda: [0.0, 0])
        self._operations: defaultdict[tuple[int, str], list] = defaultdict(
            lambda: [0.0, 0]
        )
        self._running: dict[types.FrameType, list] = {}

    def run(self, code: types.CodeType, namespace: dict) -> None:
        previous = sys.gettrace()
        sys.settrace(self._call)
        try:
            exec(code, namespace)
        finally:
            sys.settrace(previous)
            now = time.perf_counter()
            for line, since in self._running.values():
                if line is not None:
                    self._lines[line][0] += now - since
            self._running.clear()

    def result(self) -> dict[str, Any]:
        operations: defaultdict[int, list] = defaultdict(list)
        for (line, name), (seconds, calls) in self._operations.items():
            operations[line].append(
                {"line": line, "name": name, "seconds": seconds, "calls": calls}
            )
        slowest = []
        for line in sorted(operations):
            ranked = sorted(
                operations[line], key=lambda op: op["seconds"], reverse=True
            )
            slowest.extend(ranked[:OPERATIONS_PER_LINE])
        return {
            "lines": [
                {"line": line, "seconds": seconds, "hits": hits}
                for line, (seconds, hits) in sorted(self._lines.items())
                if hits
            ],
            "operations": slowest,
        }

    def _call(self, frame: types.FrameType, event: str, arg: Any):
        if event != "call":
            return None
        code = frame.f_code
        if code.co_filename == self.filename:
            self._running[frame] = [None, time.perf_counter()]
            return self._line
        caller = frame.f_back
        if (
            caller is not None
            and caller.f_code.co_filename == self.filename
            and os.path.abspath(code.co_filename) == _SYNTH_FILE
        ):
            return self._operation(frame, caller.f_lineno, code)
        return None

    def _line(self, frame: types.FrameType, event: str, arg: Any):
        now = time.perf_counter()
        running = self._running.get(frame)
        if running is None:
            return self._line
        line, since = running
        if line is not None:
            self._lines[line][0] += now - since
        running[1] = now
        if event == "line":
            running[0] = frame.f_lineno
            self._lines[frame.f_lineno][1] += 1
        elif event == "return":
            del self._running[frame]
        return self._line

    def _operation(self, frame: types.FrameType, line: int, code: types.CodeType):
        frame.f_trace_lines = False
        name = getattr(code, "co_qualname", None)
        if name is None:
            owner = frame.f_locals.get("self")
            name = code.co_name
            if owner is not None:
                name = f"{type(owner).__name__}.{name}"
        started = time.perf_counter()

        def returned(frame: types.FrameType, event: str, arg: Any):
            if event == "return":
                stats = self._operations[(line, name)]
                stats[0] += time.perf_counter() - started
                stats[1] += 1
            return returned

        return returned
