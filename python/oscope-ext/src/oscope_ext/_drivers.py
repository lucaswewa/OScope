"""Drivers: cameras, stages and lights that an extension drives (ADR-0036).

OScope wraps each in a driver of its HAL. It checks settings, targets, speeds
and lights against the driver's capabilities, refuses what can't change
while a camera streams, times and numbers frames, waits for moves to end,
and keeps the track of positions. So a driver only applies, starts, reports
and captures.
"""

from __future__ import annotations

from typing import Any

import numpy as np

KINDS = ("camera", "stage", "illumination", "importer", "exporter")

_DRIVERS: dict[str, tuple[str, type]] = {}
_INSTANCES: dict[str, Any] = {}


def driver(name: str, *, kind: str):
    """Declares a class as the driver ``name``, of ``kind``: ``camera``,
    ``stage`` or ``illumination``.

    OScope makes one instance, with no arguments, when it first needs it.
    It calls its methods with JSON: no argument when there is nothing to
    pass, or one, the JSON of the HAL's types. They return JSON the same way.

    - A camera: ``capabilities()``, ``configure(settings)``,
      ``start(settings)``, ``capture()`` (the next frame, as an array laid
      out as the settings say), ``stop()``, and ``temperature()`` if it has
      a thermometer.
    - A stage: ``capabilities()``, ``status()`` (``{"state": "idle"}``…),
      ``position()`` (``{"x", "y", "z"}`` in µm), ``move_to({"target",
      "speed"})``, ``jog({"axis", "towards_max", "speed"})``, ``stop()``,
      ``home()``, ``clear_alarm()``, ``arm_trigger({"axis", "positions"})``.
    - Lights: ``capabilities()``, ``state()``, ``set(light)``.
    - An importer (ADR-0037): ``format()`` (``{"name", "extensions"}``),
      ``probe({"path"})`` (``{"width", "height", "channels", "planes",
      "sample"}``, ``sample`` one of ``u8``, ``u16`` and ``f32``, and
      optionally ``pixel_size`` and ``channel_names``), and
      ``read({"path", "plane"})`` (the plane, as an array of its samples).
    - An exporter: ``format()``, ``begin({"path", "image", "name"})``,
      ``write({"plane"}, pixels)`` (the plane's samples, little-endian, as a
      flat array of bytes), and ``end()``.
    """
    if kind not in KINDS:
        raise ValueError(f"a driver is one of {', '.join(KINDS)}, not {kind!r}")

    def declare(cls: type) -> type:
        if name in _DRIVERS:
            raise ValueError(f"the driver {name} is declared twice")
        _DRIVERS[name] = (kind, cls)
        return cls

    return declare


def drivers() -> list[dict[str, str]]:
    """The drivers declared so far, as the description lists them."""
    return [{"name": name, "kind": kind} for name, (kind, _) in _DRIVERS.items()]


def _instance(name: str) -> Any:
    if name not in _DRIVERS:
        raise KeyError(f"no driver {name} is declared")
    if name not in _INSTANCES:
        _INSTANCES[name] = _DRIVERS[name][1]()
    return _INSTANCES[name]


def call(name: str, method: str, params: Any) -> Any:
    """Calls the driver ``name``'s method ``method``: with ``params``, unless
    it is ``None``."""
    target = getattr(_instance(name), method, None)
    if target is None or method.startswith("_"):
        raise AttributeError(f"the driver {name} has no method {method}")
    return target() if params is None else target(params)


def read(name: str, params: Any) -> np.ndarray:
    """The importer ``name``'s plane, as ``params`` ask, as bytes in an
    array, little-endian."""
    plane = np.ascontiguousarray(_instance(name).read(params))
    if plane.dtype.byteorder == ">":
        plane = plane.astype(plane.dtype.newbyteorder("<"))
    return plane.reshape(-1).view(np.uint8)


def write(name: str, params: Any, pixels: np.ndarray) -> None:
    """Gives the exporter ``name`` a plane's bytes, as ``params`` say."""
    _instance(name).write(params, pixels)


def capture(name: str) -> np.ndarray:
    """The camera driver ``name``'s next frame, as bytes in an array."""
    frame = np.ascontiguousarray(_instance(name).capture())
    return frame.reshape(-1).view(np.uint8)
