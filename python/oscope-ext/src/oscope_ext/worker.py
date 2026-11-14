"""The worker that OScope starts for each Python extension (ADR-0020), as
``python -m oscope_ext.worker <module>``, in the extension's folder and
environment.

It imports the module, whose nodes declare themselves with
:func:`oscope_ext.node`, then answers the host's messages until stdin closes.
Each message is a little-endian ``u32`` length, then that many bytes of
UTF-8 JSON, both ways:

- ``{"ping": {}}``: ``{"pong": {}}``.
- ``{"describe": {}}``: ``{"nodes": [...], "drivers": [...]}``, each node
  as the pipeline describes it, each driver its name and kind.
- ``{"driver": {"name", "method", "params"}}``: ``{"result": ...}`` (ADR-0036).
- ``{"capture": {"name", "segment", "bytes"}}``: the camera's next frame,
  written into the segment, then ``{"result": null}``.
- ``{"read": {"name", "params", "segment", "bytes"}}``: an importer's plane,
  written into the segment, then ``{"result": null}`` (ADR-0037).
- ``{"write": {"name", "params", "segment", "bytes"}}``: the plane in the
  segment, given to an exporter, then ``{"result": null}``.
- ``{"run": {"node": id, "params": {...}, "inputs": [...]}}``, each input
  ``{"segment", "width", "height", "channels"}``: for each output in turn, the
  worker asks ``{"output": {"port", "width", "height", "channels"}}``, and the
  host answers ``{"segment": name}``. Then ``{"done": {}}``, or
  ``{"error": text, "traceback": text}``.

A node's prints go to stderr, which the host logs: stdout is the messages'.
"""

from __future__ import annotations

import importlib
import json
import struct
import sys
import traceback
from typing import Any, BinaryIO

from . import _drivers, _nodes
from ._shm import Segment


def read(incoming: BinaryIO) -> dict[str, Any] | None:
    """The next message, or ``None`` once the host has closed the stream."""
    head = incoming.read(4)
    if len(head) < 4:
        return None
    (length,) = struct.unpack("<I", head)
    body = incoming.read(length)
    return json.loads(body) if len(body) == length else None


def write(outgoing: BinaryIO, message: dict[str, Any]) -> None:
    """Sends a message."""
    body = json.dumps(message).encode()
    outgoing.write(struct.pack("<I", len(body)) + body)
    outgoing.flush()


def serve(incoming: BinaryIO, outgoing: BinaryIO) -> None:
    """Answers the host's messages until it closes ``incoming``."""
    while (message := read(incoming)) is not None:
        if "ping" in message:
            write(outgoing, {"pong": {}})
        elif "describe" in message:
            nodes = [node.descriptor() for node in _nodes.nodes()]
            write(outgoing, {"nodes": nodes, "drivers": _drivers.drivers()})
        elif any(key in message for key in ("driver", "capture", "read", "write")):
            try:
                write(outgoing, {"result": drive(message)})
            except Exception as error:
                failed = {"error": f"{type(error).__name__}: {error}", "traceback": traceback.format_exc()}
                write(outgoing, failed)
        elif "run" in message:
            try:
                run(message["run"], incoming, outgoing)
            except Exception as error:
                failed = {"error": f"{type(error).__name__}: {error}", "traceback": traceback.format_exc()}
                write(outgoing, failed)
        else:
            write(outgoing, {"error": f"an unknown message: {sorted(message)}"})


def drive(message: dict[str, Any]) -> Any:
    """A driver's call; a camera's capture or an importer's plane into the
    host's segment; or the plane in the host's segment to an exporter."""
    if "driver" in message:
        call = message["driver"]
        return _drivers.call(call["name"], call["method"], call.get("params"))
    if "write" in message:
        request = message["write"]
        segment = Segment(request["segment"], request["bytes"], write=False)
        try:
            pixels = segment.bytes(request["bytes"]).copy()
        finally:
            segment.close()
        _drivers.write(request["name"], request.get("params"), pixels)
        return None
    if "read" in message:
        request = message["read"]
        data, what = _drivers.read(request["name"], request.get("params")), ("a plane", "its probe makes")
    else:
        request = message["capture"]
        data, what = _drivers.capture(request["name"]), ("a frame", "the settings make")
    if data.nbytes != request["bytes"]:
        raise ValueError(f"{what[0]} of {data.nbytes} bytes, where {what[1]} {request['bytes']}")
    segment = Segment(request["segment"], data.nbytes, write=True)
    try:
        segment.bytes(data.nbytes)[...] = data
    finally:
        segment.close()
    return None


def run(request: dict[str, Any], incoming: BinaryIO, outgoing: BinaryIO) -> None:
    """Runs a node on the images in the host's segments, and puts its
    outputs in segments the host makes."""
    node = _nodes.get(request["node"])
    segments: list[Segment] = []
    arrays = []
    try:
        for image in request["inputs"]:
            height, width, channels = image["height"], image["width"], image["channels"]
            segment = Segment(image["segment"], 4 * height * width * channels, write=False)
            segments.append(segment)
            arrays.append(segment.array(height, width, channels))
        outputs = node.run(arrays, request.get("params") or {})
        for port, output in enumerate(outputs):
            height, width, channels = output.shape
            shape = {"port": port, "width": width, "height": height, "channels": channels}
            write(outgoing, {"output": shape})
            answer = read(incoming)
            if answer is None or "segment" not in answer:
                raise RuntimeError(f"the host made no segment for output {port}: {answer}")
            segment = Segment(answer["segment"], output.nbytes, write=True)
            segments.append(segment)
            segment.array(height, width, channels)[...] = output
        write(outgoing, {"done": {}})
    finally:
        arrays.clear()
        outputs = None
        for segment in segments:
            segment.close()


def main() -> None:
    """Imports the module named on the command line, then serves."""
    incoming, outgoing = sys.stdin.buffer, sys.stdout.buffer
    sys.stdout = sys.stderr
    importlib.import_module(sys.argv[1])
    serve(incoming, outgoing)


if __name__ == "__main__":
    main()
