"""Turn the MusicScript Tracks and Beats a script creates into scene nodes for the GUI."""

from __future__ import annotations

from collections.abc import Callable, Iterable, Mapping
from dataclasses import asdict, dataclass, field
import itertools
import logging
import time
from typing import Any

from . import synth
from .provenance import creation_line
from .synth import SAMPLE_RATE, Beat, NoteEvent, Track
from .tessellation import GeometryCache, Tolerance, TopologyIndex

log = logging.getLogger(__name__)

Color = tuple[float, float, float]

NAMED_COLORS: dict[str, Color] = {
    "red": (0.95, 0.28, 0.32),
    "green": (0.28, 0.88, 0.52),
    "blue": (0.26, 0.65, 0.96),
    "yellow": (0.96, 0.86, 0.25),
    "orange": (0.98, 0.58, 0.20),
    "purple": (0.68, 0.38, 0.96),
    "cyan": (0.22, 0.84, 0.96),
    "magenta": (0.95, 0.36, 0.78),
    "pink": (0.96, 0.48, 0.76),
    "white": (0.95, 0.95, 0.96),
    "gray": (0.55, 0.58, 0.62),
    "grey": (0.55, 0.58, 0.62),
    "gold": (0.96, 0.78, 0.22),
    "steelblue": (0.27, 0.51, 0.71),
}


def identity_matrix() -> list[float]:
    return [
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ]


def is_cq_object(obj: Any) -> bool:
    return isinstance(obj, (Track, Beat))


def find_cq_objects(namespace: Mapping[str, Any]) -> dict[str, ShownObject]:
    """Every Track or Beat registered or bound to a public name."""
    reg_tracks = synth.get_registered_tracks()
    if reg_tracks:
        out: dict[str, ShownObject] = {}
        for idx, tr in enumerate(reg_tracks):
            name = tr.name or f"Track {idx + 1}"
            if name in out:
                name = f"{name} ({idx + 1})"
            out[name] = ShownObject(tr, line=creation_line(tr))
        return out

    found: dict[str, ShownObject] = {}
    for name, value in namespace.items():
        if name.startswith("_"):
            continue
        if isinstance(value, Beat):
            for idx, tr in enumerate(value.tracks):
                tname = tr.name or f"{name}_{idx + 1}"
                found[tname] = ShownObject(tr, line=creation_line(tr) or creation_line(value))
        elif isinstance(value, Track):
            found[name] = ShownObject(value, line=creation_line(value))
    return found


def to_compound(obj: Any) -> list[Track]:
    """Normalize a Track, Beat, or list of Tracks into a flat list of Tracks."""
    tracks: list[Track] = []
    items = obj if isinstance(obj, (list, tuple)) else [obj]
    for item in items:
        if isinstance(item, Beat):
            tracks.extend(item.tracks)
        elif isinstance(item, Track):
            tracks.append(item)
    return tracks


def parse_color(value: Any) -> tuple[Color, float | None]:
    """Normalize a user color specification to sRGB floats plus optional alpha."""
    if isinstance(value, str):
        text = value.strip()
        if text.startswith("#"):
            digits = text[1:]
            if len(digits) in (3, 4):
                digits = "".join(c * 2 for c in digits)
            if len(digits) not in (6, 8):
                raise ValueError(f"Invalid hex color {value!r}")
            channels = [
                int(digits[i : i + 2], 16) / 255 for i in range(0, len(digits), 2)
            ]
            alpha = channels[3] if len(channels) == 4 else None
            return (channels[0], channels[1], channels[2]), alpha
        lower = text.lower()
        if lower in NAMED_COLORS:
            return NAMED_COLORS[lower], None
        raise ValueError(f"Unknown color name {value!r}")

    if isinstance(value, (tuple, list)) and len(value) in (3, 4):
        if all(isinstance(c, int) for c in value) and any(c > 1 for c in value):
            channels = [c / 255 for c in value]
        else:
            channels = [float(c) for c in value]
        if any(c > 1.0 for c in channels[:3]):
            channels = [c / 255.0 for c in channels]
        alpha = channels[3] if len(channels) == 4 else None
        return (channels[0], channels[1], channels[2]), alpha

    raise ValueError(f"Unsupported color specification {value!r}")


@dataclass
class DisplayOptions:
    """The ``options`` mapping accepted by ``show_object`` / ``show_track``."""

    color: Color | None = None
    alpha: float | None = None
    edge_color: Color | None = None

    @classmethod
    def parse(cls, options: Mapping[str, Any] | None) -> DisplayOptions:
        result = cls()
        if not options:
            return result
        try:
            if "rgba" in options:
                r, g, b, a = options["rgba"]
                result.color, _ = parse_color((r, g, b))
                result.alpha = float(a)
            if "color" in options:
                result.color, alpha = parse_color(options["color"])
                if alpha is not None and alpha < 1.0 and result.alpha is None:
                    result.alpha = alpha
            if "alpha" in options:
                result.alpha = float(options["alpha"])
            if "edgecolor" in options:
                result.edge_color, _ = parse_color(options["edgecolor"])
        except (TypeError, ValueError) as error:
            log.warning("Ignoring invalid display option: %s", error)
        if result.alpha is not None:
            result.alpha = min(max(result.alpha, 0.0), 1.0)
        return result

    def to_message(self) -> dict[str, Any]:
        return {
            "color": list(self.color) if self.color else None,
            "alpha": self.alpha,
            "edge_color": list(self.edge_color) if self.edge_color else None,
        }


@dataclass
class ShownObject:
    obj: Any
    options: Mapping[str, Any] = field(default_factory=dict)
    line: int | None = None


@dataclass(frozen=True)
class PartSource:
    obj: Any
    location: Any = None


@dataclass
class RegistryEntry:
    obj: Track
    shape: Track | None
    children: list[str] = field(default_factory=list)
    topologies: list[TopologyIndex] = field(default_factory=list)
    sources: list[PartSource] = field(default_factory=list)
    selector_features: dict[tuple[int, str], Any] = field(default_factory=dict)

    def subshape(self, part: int, kind: str, identifier: int) -> tuple[Track, NoteEvent]:
        try:
            topology = self.topologies[part]
        except IndexError:
            raise IndexError(f"Unknown part {part}") from None
        return self.obj, topology.get(kind, identifier)


class Registry:
    """Keeps the MusicScript Track objects behind scene node keys alive."""

    def __init__(self, prefix: str = "n") -> None:
        self._prefix = prefix
        self._entries: dict[str, RegistryEntry] = {}
        self._counter = itertools.count(1)

    def add(self, entry: RegistryEntry) -> str:
        key = f"{self._prefix}{next(self._counter)}"
        self._entries[key] = entry
        return key

    def get(self, key: str) -> RegistryEntry:
        try:
            return self._entries[key]
        except KeyError:
            raise KeyError(
                f"Unknown track {key!r}; it may have been cleared "
                "or come from a previous Python session (render again)"
            ) from None

    def release(self, keys: Iterable[str]) -> None:
        for key in keys:
            entry = self._entries.pop(key, None)
            if entry is not None:
                self.release(entry.children)

    def keys(self) -> list[str]:
        return list(self._entries)

    def clear(self) -> None:
        self._entries.clear()

    def subtree_shapes(self, keys: Iterable[str]) -> list[Track]:
        tracks: list[Track] = []
        seen: set[str] = set()

        def visit(key: str) -> None:
            if key in seen:
                return
            seen.add(key)
            entry = self.get(key)
            if entry.shape is not None:
                tracks.append(entry.shape)
            for child in entry.children:
                visit(child)

        for key in keys:
            visit(key)
        return tracks


@dataclass
class SceneStats:
    identify: float = 0.0
    tessellate: float = 0.0
    encode: float = 0.0
    meshed: int = 0
    cached: int = 0
    referenced: int = 0
    mesh_bytes: int = 0

    def to_message(self) -> dict[str, Any]:
        return asdict(self)


class BuildCancelled(Exception):
    pass


def count_parts(obj: Any) -> int:
    if isinstance(obj, Beat):
        return max(1, len(obj.tracks))
    return 1


class SceneBuilder:
    def __init__(
        self,
        registry: Registry,
        tolerance: Tolerance,
        merge_assemblies: bool = False,
        geometry_cache: GeometryCache | None = None,
        known_geometry: Iterable[str] = (),
    ) -> None:
        self.registry = registry
        self.tolerance = tolerance
        self.merge_assemblies = merge_assemblies
        self.geometry_cache = geometry_cache or GeometryCache()
        self.known_geometry = set(known_geometry)
        self._source_geometry: dict[int, str] = {}
        self.stats = SceneStats()
        self.progress: Callable[[int, int], None] | None = None
        self.cancelled: Callable[[], bool] | None = None
        self._done = 0
        self._total = 0
        self._lane_index = 0

    def build(self, objects: Mapping[str, ShownObject]) -> list[dict[str, Any]]:
        self._total = sum(count_parts(shown.obj) for shown in objects.values())
        self._done = 0
        self._lane_index = 0
        if self.progress is not None:
            self.progress(0, self._total)
        registered = set(self.registry.keys())
        try:
            return self._build(objects)
        except (BuildCancelled, KeyboardInterrupt):
            self.registry.release(set(self.registry.keys()) - registered)
            raise

    def _build(self, objects: Mapping[str, ShownObject]) -> list[dict[str, Any]]:
        nodes: list[dict[str, Any]] = []
        used_names: dict[str, int] = {}

        def unique_name(base: str) -> str:
            if base not in used_names:
                used_names[base] = 1
                return base
            used_names[base] += 1
            return f"{base} #{used_names[base]}"

        for name, shown in objects.items():
            raw_obj = shown.obj
            if isinstance(raw_obj, Beat):
                tracks = list(raw_obj.tracks)
            elif isinstance(raw_obj, Track):
                tracks = [raw_obj]
            elif isinstance(raw_obj, (list, tuple)):
                tracks = [t for t in raw_obj if isinstance(t, Track)]
            else:
                log.error("Unsupported object shown: %s", type(raw_obj).__name__)
                continue

            for tr in tracks:
                tname = unique_name(tr.name if isinstance(raw_obj, Beat) else (tr.name or name))
                opts = DisplayOptions.parse(shown.options)
                if opts.color is None:
                    opts.color = tr.color
                try:
                    node = self.node(
                        tname,
                        tr,
                        opts,
                        shown_line=creation_line(tr) or shown.line,
                    )
                except BuildCancelled:
                    raise
                except Exception as error:
                    log.error(
                        "Cannot synthesize track %s: %s: %s",
                        tname,
                        type(error).__name__,
                        error,
                    )
                    continue
                if node is not None:
                    nodes.append(node)
        return nodes

    def node(
        self,
        name: str,
        track: Track,
        options: DisplayOptions,
        shown_line: int | None = None,
    ) -> dict[str, Any] | None:
        if self.cancelled is not None and self.cancelled():
            raise BuildCancelled()

        lane = self._lane_index
        self._lane_index += 1

        topology = TopologyIndex.build(track)
        part = self._mesh_track_part(track, lane, options.color, options.alpha)
        self._done += 1
        if self.progress is not None:
            self.progress(min(self._done, self._total), self._total)

        key = self.registry.add(
            RegistryEntry(
                obj=track,
                shape=track,
                topologies=[topology],
                sources=[PartSource(track, None)],
            )
        )

        line = creation_line(track) or shown_line
        fx_names = [fx.kind for fx in track.effects]
        fx_summary = " → ".join(fx_names) if fx_names else "Dry"

        return {
            "key": key,
            "name": str(name),
            "kind": f"Track ({track.instrument})",
            "line": line,
            "inspectable": True,
            **options.to_message(),
            "parts": [part],
            "children": [],
            # MusicScript Beat / Audio metadata
            "instrument": track.instrument,
            "bpm": round(float(track.bpm), 2),
            "steps": int(track.total_steps),
            "bars": int(track.bars),
            "duration": round(float(track.duration), 4),
            "volume": round(float(track.volume), 3),
            "pan": round(float(track.pan), 3),
            "muted": bool(track.muted),
            "solo": bool(track.solo),
            "sample_rate": SAMPLE_RATE,
            "fx_summary": fx_summary,
            "notes": [ev.to_dict() for ev in track.events],
            "waveform": track.compute_waveform_peaks(bins=240),
            "spectrum": track.compute_spectrum_bands(num_bands=16),
            "audio_pcm": track.encode_pcm16_base64(sample_rate=SAMPLE_RATE),
        }

    def _mesh_track_part(
        self,
        track: Track,
        lane_index: int,
        color: Color | None,
        alpha: float | None,
    ) -> dict[str, Any]:
        stats = self.stats
        started = time.perf_counter()
        geometry_id = self.geometry_cache.identifier(track, lane_index)
        geometry_id = self.geometry_cache.canonical_id(geometry_id)
        stats.identify += time.perf_counter() - started

        if geometry_id in self.known_geometry:
            mesh_message: dict[str, Any] = {"id": geometry_id}
            stats.referenced += 1
        else:
            started = time.perf_counter()
            cached = self.geometry_cache.get(geometry_id) is not None
            _, mesh = self.geometry_cache.tessellate(track, lane_index, geometry_id)
            stats.tessellate += time.perf_counter() - started
            started = time.perf_counter()
            mesh_message = mesh.to_message(geometry_id)
            stats.encode += time.perf_counter() - started
            if cached:
                stats.cached += 1
            else:
                stats.meshed += 1
            stats.mesh_bytes += mesh.byte_size()
            self.known_geometry.add(geometry_id)

        return {
            "mesh": mesh_message,
            "transform": identity_matrix(),
            "color": list(color) if color else None,
            "alpha": alpha,
        }
