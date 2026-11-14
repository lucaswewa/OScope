"""The worker's messages, through streams in memory, and segments made here as
the host makes them: a round trip through shared memory."""

import io
import mmap
import os
import sys
import uuid

import numpy as np

from oscope_ext import node, worker


@node("test.double")
def double(image):
    return image * 2


class HostSegment:
    """A segment, made as OScope makes one (``oscope-shm``)."""

    def __init__(self, size: int) -> None:
        name = f"oscope-pytest-{os.getpid()}-{uuid.uuid4().hex[:8]}"
        if sys.platform == "win32":
            self.os_name = f"Local\\{name}"
            self.map = mmap.mmap(-1, size, tagname=self.os_name)
        else:
            self.os_name = f"/dev/shm/{name}"
            with open(self.os_name, "w+b") as file:
                file.truncate(size)
                self.map = mmap.mmap(file.fileno(), size)

    def close(self) -> None:
        self.map.close()
        if sys.platform != "win32":
            os.unlink(self.os_name)


def stream(*messages):
    """A stream of ``messages``, as the host sends them."""
    sent = io.BytesIO()
    for message in messages:
        worker.write(sent, message)
    sent.seek(0)
    return sent


def replies(sent: io.BytesIO) -> list:
    """Every message in ``sent``."""
    sent.seek(0)
    found = []
    while (message := worker.read(sent)) is not None:
        found.append(message)
    return found


def test_it_answers_pings_and_describes_its_nodes():
    outgoing = io.BytesIO()
    worker.serve(stream({"ping": {}}, {"describe": {}}, {"what": {}}), outgoing)
    pong, described, unknown = replies(outgoing)
    assert pong == {"pong": {}}
    assert "test.double" in [node["id"] for node in described["nodes"]]
    assert unknown == {"error": "an unknown message: ['what']"}


def test_a_run_reads_and_writes_the_host_s_segments():
    image = np.arange(24, dtype=np.float32).reshape(2, 4, 3)
    source, target = HostSegment(image.nbytes), HostSegment(image.nbytes)
    try:
        source.map[: image.nbytes] = image.tobytes()
        input = {"segment": source.os_name, "width": 4, "height": 2, "channels": 3}
        run = {"run": {"node": "test.double", "params": {}, "inputs": [input]}}
        outgoing = io.BytesIO()
        worker.serve(stream(run, {"segment": target.os_name}), outgoing)
        asked, done = replies(outgoing)
        assert asked == {"output": {"port": 0, "width": 4, "height": 2, "channels": 3}}
        assert done == {"done": {}}
        made = np.frombuffer(target.map, dtype=np.float32, count=24).reshape(2, 4, 3).copy()
        assert np.array_equal(made, image * 2)
    finally:
        source.close()
        target.close()


def test_a_failing_run_says_why_and_the_worker_goes_on():
    outgoing = io.BytesIO()
    worker.serve(stream({"run": {"node": "test.missing", "inputs": []}}, {"ping": {}}), outgoing)
    failed, pong = replies(outgoing)
    assert failed["error"] == "KeyError: 'no node test.missing is declared'"
    assert "Traceback" in failed["traceback"]
    assert pong == {"pong": {}}
