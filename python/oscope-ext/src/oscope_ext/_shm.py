"""Shared memory: the host's segments, mapped as NumPy arrays without a copy
(ADR-0020).

The Linux host makes each segment as a file, ``/dev/shm/oscope-…``. The worker
maps it while it reads or writes it, then lets it go.
"""

from __future__ import annotations

import mmap

import numpy as np


class Segment:
    """A segment the host made, mapped while this is open."""

    def __init__(self, name: str, size: int, *, write: bool) -> None:
        access = mmap.ACCESS_WRITE if write else mmap.ACCESS_READ
        size = max(size, 1)
        self._file = open(name, "r+b" if write else "rb")
        self._map = mmap.mmap(self._file.fileno(), size, access=access)

    def array(self, height: int, width: int, channels: int) -> np.ndarray:
        """Its first samples, as an array of float32 shaped ``(height, width,
        channels)``, sharing the segment's memory."""
        count = height * width * channels
        return np.frombuffer(self._map, dtype=np.float32, count=count).reshape(height, width, channels)

    def bytes(self, count: int) -> np.ndarray:
        """Its first ``count`` bytes, as an array sharing its memory."""
        return np.frombuffer(self._map, dtype=np.uint8, count=count)

    def close(self) -> None:
        """Unmaps it. An array still sharing it keeps it mapped until the
        array goes."""
        try:
            self._map.close()
        except BufferError:
            pass
        if self._file is not None:
            self._file.close()
