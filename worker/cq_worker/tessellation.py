"""Triangulate MusicScript Tracks into compact 3D Piano Roll / Sequencer meshes for the GUI."""

from __future__ import annotations

import base64
from collections import OrderedDict
from dataclasses import dataclass
from functools import cache
import hashlib
import struct
from typing import Any

import numpy as np

from .environment import ocp_version
from .synth import NoteEvent, Track

MESH_FORMAT = 2
DEFAULT_CACHE_BYTES = 256 * 1024 * 1024


class TessellationError(RuntimeError):
    pass


@dataclass(frozen=True)
class Tolerance:
    """Mesh quality settings."""

    linear: float = 1e-3
    angular: float = 0.2


@dataclass
class Mesh:
    positions: np.ndarray          # (n, 3) float32
    normals: np.ndarray            # (n, 3) float32
    indices: np.ndarray            # (m,) uint32, three per triangle
    edges: np.ndarray              # (k, 6) float32, one line segment per row
    points: np.ndarray             # (p, 3) float32
    face_positions: np.ndarray     # (n, 3) float32
    face_indices: np.ndarray       # (m,) uint32
    triangle_faces: np.ndarray     # (m / 3,) uint32, 1-based note/step id per triangle
    segment_edges: np.ndarray      # (k,) uint32, 1-based note/step id per segment
    topology_vertices: np.ndarray  # (v, 3) float32

    def is_empty(self) -> bool:
        return not (len(self.indices) or len(self.edges) or len(self.points))

    def bounds(self) -> tuple[list[float], list[float]]:
        coords = [
            a.reshape(-1, 3)
            for a in (self.positions, self.edges, self.points)
            if a.size
        ]
        if not coords:
            return [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]
        stacked = np.concatenate(coords)
        return stacked.min(axis=0).tolist(), stacked.max(axis=0).tolist()

    def byte_size(self) -> int:
        return sum(
            array.nbytes
            for array in (
                self.positions,
                self.normals,
                self.indices,
                self.edges,
                self.points,
                self.face_positions,
                self.face_indices,
                self.triangle_faces,
                self.segment_edges,
                self.topology_vertices,
            )
        )

    def to_message(self, geometry_id: str = "") -> dict[str, Any]:
        lo, hi = self.bounds()
        return {
            "id": geometry_id,
            "positions": _encode(self.positions, "<f4"),
            "normals": _encode(self.normals, "<f4"),
            "indices": _encode(self.indices, "<u4"),
            "edges": _encode(self.edges, "<f4"),
            "points": _encode(self.points, "<f4"),
            "topology": {
                "face_positions": _encode(self.face_positions, "<f4"),
                "face_indices": _encode(self.face_indices, "<u4"),
                "triangle_faces": _encode(self.triangle_faces, "<u4"),
                "segment_edges": _encode(self.segment_edges, "<u4"),
                "vertices": _encode(self.topology_vertices, "<f4"),
            },
            "bbox": [lo, hi],
        }


@cache
def mesh_generator() -> str:
    return f"musicscript-mesh {MESH_FORMAT}; {ocp_version()}"


class GeometryCache:
    """Content-addressed LRU cache of tessellated Track meshes."""

    def __init__(
        self, max_bytes: int = DEFAULT_CACHE_BYTES, generator: str | None = None
    ) -> None:
        self.max_bytes = max(0, max_bytes)
        generator_bytes = (generator or mesh_generator()).encode()
        self._generator = struct.pack("<I", len(generator_bytes)) + generator_bytes
        self._meshes: OrderedDict[str, Mesh] = OrderedDict()
        self._aliases: dict[str, str] = {}
        self._bytes = 0

    def identifier(self, track: Track, lane_index: int = 0) -> str:
        digest = hashlib.blake2b(digest_size=16, person=b"musicscript-mesh")
        digest.update(self._generator)
        digest.update(f"{track.name}:{track.instrument}:{track.total_steps}:{lane_index}:".encode())
        for ev in track.events:
            digest.update(
                struct.pack("<fffif", ev.step, ev.duration, ev.velocity, ev.midi, float(ev.slide_to or 0))
            )
        return digest.hexdigest()

    def canonical_id(self, geometry_id: str) -> str:
        return self._aliases.get(geometry_id, geometry_id)

    def get(self, geometry_id: str) -> Mesh | None:
        geometry_id = self.canonical_id(geometry_id)
        mesh = self._meshes.get(geometry_id)
        if mesh is not None:
            self._meshes.move_to_end(geometry_id)
        return mesh

    def put(self, geometry_id: str, mesh: Mesh) -> None:
        if self.max_bytes == 0 or mesh.byte_size() > self.max_bytes:
            return
        previous = self._meshes.pop(geometry_id, None)
        if previous is not None:
            self._bytes -= previous.byte_size()
        self._meshes[geometry_id] = mesh
        self._bytes += mesh.byte_size()
        while self._bytes > self.max_bytes:
            removed_id, removed = self._meshes.popitem(last=False)
            self._bytes -= removed.byte_size()
            self._aliases = {
                alias: target
                for alias, target in self._aliases.items()
                if target != removed_id
            }

    def tessellate(
        self,
        track: Track,
        lane_index: int = 0,
        geometry_id: str | None = None,
    ) -> tuple[str, Mesh]:
        gid = self.canonical_id(geometry_id or self.identifier(track, lane_index))
        cached = self.get(gid)
        if cached is not None:
            return gid, cached
        mesh = tessellate(track, lane_index=lane_index)
        self.put(gid, mesh)
        return gid, mesh


@dataclass(frozen=True)
class TopologyIndex:
    """1-based index mapping picked faces/edges/vertices back to `NoteEvent`s on a `Track`."""

    faces: tuple[NoteEvent, ...]
    edges: tuple[NoteEvent, ...]
    vertices: tuple[NoteEvent, ...]

    @classmethod
    def build(cls, track: Track) -> TopologyIndex:
        evs = tuple(track.events)
        return cls(faces=evs, edges=evs, vertices=evs)

    def get(self, kind: str, identifier: int) -> NoteEvent:
        collection = {
            "face": self.faces,
            "edge": self.edges,
            "vertex": self.vertices,
        }.get(kind)
        if collection is None:
            raise ValueError(f"Unknown topology kind {kind!r}")
        if identifier < 1 or identifier > len(collection):
            raise IndexError(f"Unknown {kind} id {identifier}")
        return collection[identifier - 1]


def tessellate(
    track: Track,
    tolerance: Tolerance | None = None,
    lane_index: int = 0,
) -> Mesh:
    """Build a 3D Piano Roll / Sequencer mesh for `track` at `lane_index`."""
    del tolerance
    verts: list[tuple[float, float, float]] = []
    norms: list[tuple[float, float, float]] = []
    tris: list[int] = []
    tri_faces: list[int] = []
    edge_segs: list[tuple[float, float, float, float, float, float]] = []
    seg_edges: list[int] = []
    topo_verts: list[tuple[float, float, float]] = []

    step_scale = 2.0
    total_x = max(16, track.total_steps) * step_scale
    center_offset_x = -total_x * 0.5
    lane_y = -lane_index * 7.5

    def add_box(
        x0: float,
        x1: float,
        y0: float,
        y1: float,
        z0: float,
        z1: float,
        fid: int,
        eid: int,
    ) -> None:
        faces = [
            ((x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1), (0.0, 0.0, 1.0)),
            ((x0, y1, z0), (x1, y1, z0), (x1, y0, z0), (x0, y0, z0), (0.0, 0.0, -1.0)),
            ((x0, y0, z0), (x1, y0, z0), (x1, y0, z1), (x0, y0, z1), (0.0, -1.0, 0.0)),
            ((x1, y1, z0), (x0, y1, z0), (x0, y1, z1), (x1, y1, z1), (0.0, 1.0, 0.0)),
            ((x0, y1, z0), (x0, y0, z0), (x0, y0, z1), (x0, y1, z1), (-1.0, 0.0, 0.0)),
            ((x1, y0, z0), (x1, y1, z0), (x1, y1, z1), (x1, y0, z1), (1.0, 0.0, 0.0)),
        ]
        for p0, p1, p2, p3, n in faces:
            base = len(verts)
            verts.extend([p0, p1, p2, p3])
            norms.extend([n, n, n, n])
            tris.extend([base, base + 1, base + 2, base, base + 2, base + 3])
            tri_faces.extend([fid, fid])

        # Top & bottom outline edges
        corners = [
            (x0, y0, z1, x1, y0, z1),
            (x1, y0, z1, x1, y1, z1),
            (x1, y1, z1, x0, y1, z1),
            (x0, y1, z1, x0, y0, z1),
        ]
        for seg in corners:
            edge_segs.append(seg)
            seg_edges.append(eid)

    # Track bed rail (id = 0)
    add_box(
        center_offset_x,
        center_offset_x + total_x,
        lane_y - 2.2,
        lane_y + 2.2,
        -0.35,
        0.0,
        0,
        0,
    )

    for idx, ev in enumerate(track.events):
        nid = idx + 1
        x0 = center_offset_x + ev.step * step_scale + 0.08
        x1 = center_offset_x + (ev.step + max(0.25, ev.duration * 0.92)) * step_scale - 0.08
        if x1 <= x0:
            x1 = x0 + 0.25
        pitch_shift = max(-1.5, min(1.5, (ev.midi - track.base_midi) * 0.16))
        y0 = lane_y + pitch_shift - 0.85
        y1 = lane_y + pitch_shift + 0.85
        z0 = 0.05
        z1 = 0.35 + float(ev.velocity) * 3.8
        add_box(x0, x1, y0, y1, z0, z1, nid, nid)
        topo_verts.append(((x0 + x1) * 0.5, (y0 + y1) * 0.5, z1))

    return Mesh(
        positions=np.asarray(verts, dtype=np.float32) if verts else _empty(3),
        normals=np.asarray(norms, dtype=np.float32) if norms else _empty(3),
        indices=np.asarray(tris, dtype=np.uint32) if tris else np.zeros(0, dtype=np.uint32),
        edges=np.asarray(edge_segs, dtype=np.float32) if edge_segs else _empty(6),
        points=_empty(3),
        face_positions=_empty(3),
        face_indices=np.zeros(0, dtype=np.uint32),
        triangle_faces=np.asarray(tri_faces, dtype=np.uint32) if tri_faces else np.zeros(0, dtype=np.uint32),
        segment_edges=np.asarray(seg_edges, dtype=np.uint32) if seg_edges else np.zeros(0, dtype=np.uint32),
        topology_vertices=np.asarray(topo_verts, dtype=np.float32) if topo_verts else _empty(3),
    )


def _encode(array: np.ndarray, dtype: str) -> str:
    data = np.ascontiguousarray(array, dtype=dtype).tobytes()
    return base64.b64encode(data).decode("ascii")


def _empty(columns: int) -> np.ndarray:
    return np.zeros((0, columns), dtype=np.float32)
