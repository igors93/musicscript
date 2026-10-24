"""Line-level stepping debugger for the editor buffer.

Built on ``sys.settrace``: only frames executing the editor buffer are traced,
so library code (CadQuery, OCP) runs at full speed. When the debugger decides
to pause it calls back into the worker, which reports the paused state to the
GUI and blocks until the next command arrives.
"""

from __future__ import annotations

import enum
import sys
import types
from collections.abc import Callable, Iterable


class Action(enum.Enum):
    CONTINUE = "continue"
    STEP = "step"
    STEP_IN = "step_in"
    STEP_OUT = "step_out"
    STOP = "stop"


class DebugStop(BaseException):
    """Raised inside the script to end a debugging session early.

    Like ``KeyboardInterrupt``, it is not an ``Exception``: a script's own
    ``except Exception`` must not swallow the user's request to stop.
    """


class Debugger:
    """Runs code objects compiled from ``filename`` under the tracer.

    ``on_pause(frame)`` is invoked whenever execution stops at a line; it
    returns the next :class:`Action` and may update :attr:`breakpoints`.
    """

    def __init__(
        self,
        filename: str,
        breakpoints: Iterable[int],
        on_pause: Callable[[types.FrameType], Action],
    ) -> None:
        self.filename = filename
        self.breakpoints = set(breakpoints)
        self._on_pause = on_pause
        # Like the original editor, start running until the first breakpoint.
        self._action = Action.CONTINUE
        self._depth = 0

    def run(self, code: types.CodeType, namespace: dict) -> None:
        previous = sys.gettrace()
        sys.settrace(self._trace_calls)
        try:
            exec(code, namespace)
        finally:
            sys.settrace(previous)

    def _trace_calls(self, frame: types.FrameType, event: str, arg):
        if frame.f_code.co_filename != self.filename:
            return None  # don't trace library code
        return self._trace_lines

    def _trace_lines(self, frame: types.FrameType, event: str, arg):
        if event == "line" and self._should_pause(frame):
            self._pause(frame)
        return self._trace_lines

    def _should_pause(self, frame: types.FrameType) -> bool:
        if frame.f_lineno in self.breakpoints:
            return True
        depth = self._script_depth(frame)
        if self._action is Action.STEP_IN:
            return True
        if self._action is Action.STEP:
            return depth <= self._depth
        if self._action is Action.STEP_OUT:
            return depth < self._depth
        return False

    def _pause(self, frame: types.FrameType) -> None:
        self._action = self._on_pause(frame)
        self._depth = self._script_depth(frame)
        if self._action is Action.STOP:
            raise DebugStop

    def _script_depth(self, frame: types.FrameType | None) -> int:
        """Number of active frames that execute the editor buffer."""

        depth = 0
        while frame is not None:
            if frame.f_code.co_filename == self.filename:
                depth += 1
            frame = frame.f_back
        return depth
