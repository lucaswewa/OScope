"""Shared memory: the host's segments, mapped as NumPy arrays without a copy
(ADR-0020).

The host makes each segment and names it as the system does: a named file
mapping, ``Local\\oscope-…``, on Windows, and a file, ``/dev/shm/oscope-…``,
on Linux. The worker maps it while it reads or writes it, then lets it go.
"""

from __future__ import annotations

import mmap
import sys

import numpy as np


class Segment:
    """A segment the host made, mapped while this is open."""

    def __init__(self, name: str, size: int, *, write: bool) -> None:
        access = mmap.ACCESS_WRITE if write else mmap.ACCESS_READ
        size = max(size, 1)
        self._file = None
        if sys.platform == "win32":
            self._map = mmap.mmap(-1, size, tagname=name, access=access)
        else:
            self._file = open(name, "r+b" if write else "rb")
            self._map = mmap.mmap(self._file.fileno(), size, access=access)

    def array(self, height: int, width: int, channels: int) -> np.ndarray:
        """Its first samples, as an array of float32 shaped ``(height, width,
        channels)``, sharing the segment's memory."""
        count = height * width * channels
        return np.frombuffer(self._map, dtype=np.float32, count=count).reshape(height, width, channels)

    def close(self) -> None:
        """Unmaps it. An array still sharing it keeps it mapped until the
        array goes."""
        try:
            self._map.close()
        except BufferError:
            pass
        if self._file is not None:
            self._file.close()
