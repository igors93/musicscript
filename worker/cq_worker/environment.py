"""Describe the Python / MusicScript / NumPy environment used by the worker."""

from __future__ import annotations

import importlib.metadata
import platform
import sys
from typing import Any

import numpy as np


def distribution_version(name: str) -> str:
    """Return an installed distribution version without hiding bad metadata."""
    try:
        return importlib.metadata.version(name)
    except importlib.metadata.PackageNotFoundError:
        return "unknown"


def ocp_version() -> str:
    """Return the synthesis DSP backend version string."""
    return f"NumPy {np.__version__}"


def runtime_environment() -> dict[str, Any]:
    """Versions needed to reproduce worker tests, timings and failures."""
    return {
        "python": platform.python_version(),
        "implementation": platform.python_implementation(),
        "cadquery": "MusicScript 1.0",
        "ocp": ocp_version(),
        "platform": platform.platform(),
        "executable": sys.executable,
    }


def summary() -> str:
    environment = runtime_environment()
    return (
        f"Python {environment['python']} ({environment['implementation']}) · "
        f"{environment['cadquery']} · DSP {environment['ocp']} · "
        f"{environment['platform']}"
    )
