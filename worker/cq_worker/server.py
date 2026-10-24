"""Request dispatch and session state.

The worker is single threaded: requests are handled one at a time, in
arrival order. While a debugging session is paused, :meth:`Server.wait_for_debug_action`
keeps serving read-only requests (completion, inspection, export) so the GUI
stays useful, and returns as soon as a debugger action arrives.
"""

from __future__ import annotations

import logging
import os
import sys
import tempfile
import time
import traceback
import types
from modulefinder import ModuleFinder

from . import (
    completion,
    exporting,
    inspector,
    measurement,
    measurement_code,
    network,
    selection,
    synth as cq,
)
from .console import ConsoleSession
from .debugger import Action, Debugger
from .display import Displayer
from .environment import runtime_environment
from .execution import (
    SCRIPT_FILENAME,
    Execution,
    RunSettings,
    ShownObjects,
    describe_variables,
    script_api,
)
from .profiling import LineProfiler
from .protocol import (
    LOG_CAPTURE_CHARS,
    LOG_MESSAGE_CHARS,
    PROTOCOL_VERSION,
    Channel,
    Message,
    truncate_text,
)
from .scene import (
    BuildCancelled,
    Registry,
    SceneBuilder,
    find_cq_objects,
    to_compound,
)
from .tessellation import GeometryCache, Tolerance

log = logging.getLogger(__name__)

#: Seconds between two progress messages of one request.
PROGRESS_INTERVAL = 0.1

#: Requests the GUI can interrupt (``KeyboardInterrupt``) while they run.
_INTERRUPTIBLE = {"run", "console"}

#: Requests that are safe to serve while a debugging session is paused.
_ALLOWED_WHILE_PAUSED = {
    "complete",
    "inspect",
    "inspect_geometry",
    "measure",
    "selector",
    "export",
    "imports",
    "release",
    "check_updates",
}


class RequestError(Exception):
    """A request that cannot be served; reported to the GUI as an error."""


class ChannelLogHandler(logging.Handler):
    """Forwards Python log records (including the script's ``log()``) to the GUI."""

    def __init__(self, channel: Channel) -> None:
        super().__init__(logging.INFO)
        self._channel = channel
        self._request_id = channel.current_id
        self._captured_chars = 0
        self._truncated = False

    def emit(self, record: logging.LogRecord) -> None:
        try:
            request_id = self._channel.current_id
            if self._request_id != request_id:
                self._request_id = request_id
                self._captured_chars = 0
                self._truncated = False
            if self._truncated:
                return

            message = record.getMessage()
            if record.exc_info:
                message += "\n" + "".join(traceback.format_exception(*record.exc_info))
            remaining = LOG_CAPTURE_CHARS - self._captured_chars
            retained = min(LOG_MESSAGE_CHARS, remaining)
            captured = message[:retained]
            self._captured_chars += len(captured)
            if len(message) > len(captured) and remaining <= LOG_MESSAGE_CHARS:
                message = (
                    captured
                    + f"\n[log output truncated after {LOG_CAPTURE_CHARS} characters "
                    "for this request; further records were discarded]\n"
                )
                self._truncated = True
            else:
                message = truncate_text(message, LOG_MESSAGE_CHARS, "log")
            self._channel.send(
                {
                    "type": "log",
                    "id": self._channel.current_id,
                    "level": record.levelname.lower(),
                    "source": record.name,
                    "message": message,
                }
            )
        except Exception:
            self.handleError(record)


class Server:
    def __init__(self, channel: Channel, session: str | None = None) -> None:
        """``session`` identifies this worker process among the GUI's
        successive workers; it makes scene keys unique across restarts."""

        self.channel = channel
        self.registry = Registry(f"s{session}-n" if session else "n")
        self.console = ConsoleSession(Displayer(self.channel.send))
        self.geometry_cache = GeometryCache()
        #: Stack frames of inspected workplanes, by scene key.
        self.inspections: dict[str, list[cq.Workplane]] = {}
        self._debugger: Debugger | None = None
        self._handlers = {
            "run": self.run,
            "release": self.release,
            "complete": self.complete,
            "describe": self.describe,
            "inspect": self.inspect,
            "inspect_geometry": self.inspect_geometry,
            "measure": self.measure,
            "selector": self.selector,
            "measurement_code": self.measurement_code,
            "console": self.run_console,
            "console_reset": self.reset_console,
            "export": self.export,
            "upload": self.upload,
            "check_updates": self.check_updates,
            "imports": self.imports,
        }

    # -- main loop ---------------------------------------------------------

    def serve_forever(self) -> None:
        environment = runtime_environment()
        self.channel.send(
            {
                "type": "ready",
                "protocol": PROTOCOL_VERSION,
                "python": environment["python"],
                "cadquery": environment["cadquery"],
                "ocp": environment["ocp"],
                "executable": environment["executable"],
                "completion": completion.is_available(),
                "pid": os.getpid(),
            }
        )
        while True:
            try:
                request = self.channel.receive()
            except KeyboardInterrupt:
                # A Ctrl+C sent to the whole process group of a terminal;
                # only Stop decides what the worker interrupts.
                continue
            if request is None or request.get("cmd") == "shutdown":
                return
            self.dispatch(request)

    def dispatch(self, request: Message) -> None:
        request_id = request.get("id")
        previous_id, self.channel.current_id = self.channel.current_id, request_id
        try:
            handler = self._handlers.get(request.get("cmd"))
            if handler is None:
                raise RequestError(f"Unknown command {request.get('cmd')!r}")
            if request.get("cmd") in _INTERRUPTIBLE:
                with self.channel.interruptible(request_id):
                    reply = handler(request)
            else:
                reply = handler(request)
            if reply is not None:
                self.channel.send({"id": request_id, **reply})
        except KeyboardInterrupt:
            # Stopped outside the script itself (handlers catch it there).
            self.channel.send({"type": "error", "id": request_id, "message": "Stopped"})
        except RequestError as error:
            self.channel.send(
                {"type": "error", "id": request_id, "message": str(error)}
            )
        except Exception as error:
            log.debug("Request %r failed", request.get("cmd"), exc_info=True)
            message = f"{type(error).__name__}: {error}"
            self.channel.send({"type": "error", "id": request_id, "message": message})
        finally:
            sys.stdout.flush()
            sys.stderr.flush()
            self.channel.current_id = previous_id

    # -- running scripts ---------------------------------------------------

    def run(self, request: Message) -> Message:
        self.registry.release(request.get("release", []))
        builder = self._scene_builder(request)
        execution = Execution(
            request.get("script", ""),
            request.get("path"),
            RunSettings.from_message(request.get("settings")),
        )
        debug = bool(request.get("debug"))
        executor = None
        if debug:
            self._debugger = Debugger(
                SCRIPT_FILENAME,
                request.get("breakpoints", []),
                lambda frame: self._pause(frame, execution, builder),
            )
            executor = self._debugger.run
        # The debugger traces the script itself; it cannot be profiled too.
        profiler = None
        if request.get("profile") and not debug:
            profiler = LineProfiler(SCRIPT_FILENAME)
            executor = profiler.run

        started = time.perf_counter()
        try:
            ok = execution.run(executor)
        finally:
            self._debugger = None
        duration = time.perf_counter() - started

        hidden = execution.hidden_names()
        objects = variables = None
        cancelled = False
        if ok or debug:
            # A failed plain run keeps the previous model on screen, like the
            # original editor; a debugging session always shows where it ended.
            shown = execution.shown or find_cq_objects(execution.namespace)
            variables = describe_variables(execution.namespace, hidden)
            self.console.update_variables(
                {k: v for k, v in execution.namespace.items() if k not in hidden}
            )
            request_id = request.get("id")
            builder.progress = self._progress_reporter(request_id)
            builder.cancelled = lambda: self.channel.cancel_requested(request_id)
            try:
                objects = builder.build(shown)
            except (BuildCancelled, KeyboardInterrupt):
                # Like a failed run: the previous model stays on screen.
                cancelled = True
            finally:
                self.channel.forget_cancel(request_id)
        return {
            "type": "run_finished",
            "ok": ok,
            "debug": debug,
            "stopped": execution.stopped,
            "cancelled": cancelled,
            "interrupted": execution.interrupted,
            # Also after a failure: where the time went until then.
            "profile": profiler.result() if profiler else None,
            "objects": objects,
            "variables": variables,
            "error": execution.error,
            "duration": duration,
            "timings": {
                "script": duration,
                **builder.stats.to_message(),
                # Wall clock, so the GUI can tell how long the reply took
                # to serialize and arrive.
                "sent_at": time.time(),
            },
        }

    def _progress_reporter(self, request_id):
        """Sends how far meshing is, at most every :data:`PROGRESS_INTERVAL`
        seconds, and always the start and the end."""

        last = None

        def report(done: int, total: int) -> None:
            nonlocal last
            now = time.monotonic()
            if last is not None and 0 < done < total and now - last < PROGRESS_INTERVAL:
                return
            last = now
            self.channel.send(
                {
                    "type": "progress",
                    "id": request_id,
                    "stage": "meshing",
                    "done": done,
                    "total": total,
                }
            )

        return report

    def _pause(
        self, frame: types.FrameType, execution: Execution, builder: SceneBuilder
    ) -> Action:
        hidden = execution.hidden_names()
        local_names = frame.f_locals
        self.channel.send(
            {
                "type": "debug_paused",
                "id": self.channel.current_id,
                "line": frame.f_lineno,
                "variables": describe_variables(local_names, hidden),
                "objects": builder.build(find_cq_objects(local_names)),
            }
        )
        return self.wait_for_debug_action()

    def wait_for_debug_action(self) -> Action:
        while True:
            request = self.channel.receive()
            if request is None:
                return Action.STOP
            command = request.get("cmd")
            if command == "debug":
                if "breakpoints" in request and self._debugger is not None:
                    self._debugger.breakpoints = set(request["breakpoints"])
                return Action(request.get("action", "continue"))
            if command == "shutdown":
                sys.exit(0)
            if command in _ALLOWED_WHILE_PAUSED:
                self.dispatch(request)
            else:
                self.channel.send(
                    {
                        "type": "error",
                        "id": request.get("id"),
                        "message": "Not available while debugging; continue or stop first",
                    }
                )

    def release(self, request: Message) -> None:
        keys = request.get("keys", [])
        self.registry.release(keys)
        for key in keys:
            self.inspections.pop(key, None)

    # -- console -----------------------------------------------------------

    def run_console(self, request: Message) -> Message:
        try:
            picked = selection.PickedSelection.from_message(request.get("selection"))
        except measurement.MeasurementError as error:
            raise RequestError(str(error)) from None
        self.console.set_selection(lambda: picked.resolve(self.registry))
        more = self.console.execute(request.get("source", ""), request.get("id"))
        shown = self.console.take_shown()
        if shown:
            self.channel.send(
                {
                    "type": "scene_add",
                    "id": request.get("id"),
                    "objects": self._scene_builder(request).build(shown),
                }
            )
        return {"type": "console_result", "more": more}

    def reset_console(self, request: Message) -> Message:
        self.console.reset()
        return {"type": "console_result", "more": False}

    # -- editor services ---------------------------------------------------

    def complete(self, request: Message) -> Message:
        if request.get("target") == "console":
            namespace = self.console.namespace
        else:
            namespace = self._script_namespace()
        result = completion.complete(
            request.get("source", ""),
            int(request.get("line", 1)),
            int(request.get("column", 0)),
            request.get("path"),
            namespace,
        )
        return {"type": "completions", **result}

    def describe(self, request: Message) -> Message:
        """Documentation of the name under the editor's pointer."""

        result = completion.describe(
            request.get("source", ""),
            int(request.get("line", 1)),
            int(request.get("column", 0)),
            request.get("path"),
            self._script_namespace(),
        )
        return {"type": "description", **result}

    @staticmethod
    def _script_namespace() -> dict:
        return {**script_api(ShownObjects()), "__name__": "__cq_main__"}

    def imports(self, request: Message) -> Message:
        """Local modules imported by a script, so the GUI can watch them too."""

        path = request["path"]
        finder = ModuleFinder([os.path.dirname(os.path.abspath(path))])
        try:
            finder.run_script(path)
        except SyntaxError as error:
            log.warning("Syntax error in %s: %s", path, error)
        except Exception as error:
            log.warning("Cannot determine imports of %s: %s", path, error)
        paths = [
            module.__file__
            for name, module in finder.modules.items()
            if name != "__main__"
            and module.__file__
            and os.path.isfile(module.__file__)
        ]
        return {"type": "imports", "paths": sorted(set(paths))}

    # -- inspection --------------------------------------------------------

    def inspect(self, request: Message) -> Message:
        key = request["key"]
        obj = self.registry.get(key).obj
        if not isinstance(obj, (cq.Track, cq.Beat)):
            raise RequestError("Only MusicScript Tracks and Beats can be inspected")
        frames = inspector.stack_frames(obj)
        self.inspections[key] = frames
        return {
            "type": "inspection",
            "key": key,
            "frames": inspector.describe_stack(frames),
        }

    def inspect_geometry(self, request: Message) -> Message:
        frames = self.inspections.get(request["key"])
        if frames is None:
            raise RequestError("Object is no longer inspected")
        frame = frames[int(request["frame"])]
        item = request.get("item")
        geometry = inspector.frame_geometry(
            frame, None if item is None else int(item), self._tolerance(request)
        )
        return {"type": "inspection_geometry", **geometry}

    def measure(self, request: Message) -> Message:
        try:
            result = measurement.measure(
                self.registry, request.get("selections"), request.get("objects")
            )
        except measurement.MeasurementError as error:
            raise RequestError(str(error)) from None
        return {"type": "measurement", **result}

    def measurement_code(self, request: Message) -> Message:
        """Code repeating a measurement with the script's variables, which
        the console holds after a run."""

        try:
            result = measurement_code.measurement_code(
                self.registry,
                self.console.namespace,
                request.get("selections"),
                request.get("objects"),
            )
        except measurement.MeasurementError as error:
            raise RequestError(str(error)) from None
        return {"type": "measurement_code", **result}

    def selector(self, request: Message) -> Message:
        try:
            result = selection.selector_candidates(
                self.registry, request.get("selections")
            )
        except measurement.MeasurementError as error:
            raise RequestError(str(error)) from None
        return {"type": "selector", **result}

    # -- files and network -------------------------------------------------

    def export(self, request: Message) -> Message:
        exporting.export(
            self.registry,
            request.get("keys", []),
            request["format"],
            request["path"],
            float(request.get("tolerance", 0.1)),
            float(request.get("angular_tolerance", 0.1)),
        )
        return {"type": "export_finished", "path": request["path"]}

    def upload(self, request: Message) -> Message:
        file_type = request.get("file_type", "step").lower()
        if file_type == "py":
            data = request.get("source", "").encode("utf-8")
        else:
            shapes = self.registry.subtree_shapes(request.get("keys", []))
            if not shapes:
                raise RequestError("Please select a model to upload")
            data = _export_to_bytes(to_compound(shapes), file_type)
        result = network.upload(request["url"], data, f"model.{file_type}")
        return {"type": "upload_finished", **result}

    def check_updates(self, request: Message) -> Message:
        """Newer MusicScript releases."""

        version_str = "1.0.0"
        reply = {"type": "updates", "current": version_str, "newer": []}
        editor = request.get("editor")
        if editor:
            reply["editor"] = {
                "current": editor,
                "newer": [],
            }
        return reply

    # -- helpers -----------------------------------------------------------

    @staticmethod
    def _tolerance(request: Message) -> Tolerance:
        scene = request.get("scene") or {}
        return Tolerance(
            linear=float(scene.get("linear_tolerance", Tolerance.linear)),
            angular=float(scene.get("angular_tolerance", Tolerance.angular)),
        )

    def _scene_builder(self, request: Message) -> SceneBuilder:
        scene = request.get("scene") or {}
        return SceneBuilder(
            self.registry,
            self._tolerance(request),
            merge_assemblies=bool(scene.get("merge_assemblies", False)),
            geometry_cache=self.geometry_cache,
            known_geometry=request.get("known_geometry", ()),
        )


def _export_to_bytes(tracks: list[cq.Track], file_type: str) -> bytes:
    fd, path = tempfile.mkstemp(suffix=f".{file_type}")
    os.close(fd)
    try:
        if file_type in ("wav", "stl"):
            stereo = cq.mix_tracks_stereo(tracks)
            cq.write_wav_file(path, stereo)
        elif file_type in ("mid", "midi", "step"):
            cq.write_midi_file(path, tracks)
        else:
            raise RequestError(f"Unsupported upload type {file_type!r}")
        with open(path, "rb") as file:
            return file.read()
    finally:
        os.unlink(path)
