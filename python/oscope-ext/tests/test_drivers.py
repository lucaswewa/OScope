"""Drivers: declared and described, called with JSON, a camera's frame
written into the host's segment, and a kind no driver is refused."""

import io

import numpy as np
import pytest
from test_worker import HostSegment, replies, stream

from oscope_ext import driver, drivers, worker


@driver("test.lamp", kind="illumination")
class Lamp:
    def __init__(self):
        self.light = {"channel": "lamp", "on": False, "intensity": 0.0}

    def capabilities(self):
        return {"model": "test lamp", "channels": [{"name": "lamp", "kind": "transmitted", "dimmable": True}]}

    def state(self):
        return [self.light]

    def set(self, light):
        self.light = light


@driver("test.camera", kind="camera")
class Camera:
    def capture(self):
        return np.arange(12, dtype=np.uint16).reshape(3, 4)


def call(name, method, params=None):
    return {"driver": {"name": name, "method": method, "params": params}}


def test_drivers_are_described_and_called_with_json():
    assert {"name": "test.lamp", "kind": "illumination"} in drivers()
    light = {"channel": "lamp", "on": True, "intensity": 0.5}
    outgoing = io.BytesIO()
    sent = stream(
        call("test.lamp", "set", light),
        call("test.lamp", "state"),
        call("test.lamp", "dance"),
        {"describe": {}},
    )
    worker.serve(sent, outgoing)
    done, state, missing, described = replies(outgoing)
    assert done == {"result": None}
    assert state == {"result": [light]}
    assert missing["error"] == "AttributeError: the driver test.lamp has no method dance"
    assert {"name": "test.camera", "kind": "camera"} in described["drivers"]


def test_a_camera_s_frame_goes_into_the_host_s_segment():
    target = HostSegment(24)
    try:
        capture = {"name": "test.camera", "segment": target.os_name, "bytes": 24}
        wrong = dict(capture, bytes=10)
        outgoing = io.BytesIO()
        worker.serve(stream({"capture": capture}, {"capture": wrong}), outgoing)
        done, refused = replies(outgoing)
        assert done == {"result": None}
        frame = np.frombuffer(target.map, dtype=np.uint16, count=12).copy()
        assert np.array_equal(frame, np.arange(12))
        assert "a frame of 24 bytes, where the settings make 10" in refused["error"]
    finally:
        target.close()


def test_a_driver_of_no_known_kind_is_refused():
    with pytest.raises(ValueError, match="not 'printer'"):
        driver("test.printer", kind="printer")


@driver("test.row", kind="workflow")
class Row:
    def schema(self):
        return {"type": "object", "properties": {"tiles": {"type": "integer"}}}

    def plan(self, request):
        step = request["field"]["step_um"]["width"]
        tiles = range(request["params"]["tiles"])
        return [{"x_um": 1000.0 + k * step, "y_um": 500.0} for k in tiles]


def test_a_workflow_plans_its_tiles_from_the_field():
    assert {"name": "test.row", "kind": "workflow"} in drivers()
    field = {"cell_um": {"width": 100.0, "height": 80.0}, "step_um": {"width": 90.0, "height": 72.0}}
    outgoing = io.BytesIO()
    sent = stream(call("test.row", "plan", {"params": {"tiles": 3}, "field": field}))
    worker.serve(sent, outgoing)
    (planned,) = replies(outgoing)
    xs = [tile["x_um"] for tile in planned["result"]]
    assert xs == [1000.0, 1090.0, 1180.0]
