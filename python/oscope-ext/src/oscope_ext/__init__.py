"""The SDK of OScope's Python extensions (ADR-0020).

An extension's module declares its nodes with :func:`node`::

    import numpy as np
    from oscope_ext import node

    @node("example.invert", category="example")
    def invert(image: np.ndarray, *, level: float = 1.0) -> np.ndarray:
        \"\"\"Turns each sample into ``level`` less it.\"\"\"
        return level - image

OScope runs the module in a worker of its own (:mod:`oscope_ext.worker`),
in the extension's environment, and hands it frames through shared memory,
as NumPy arrays shaped ``(height, width, channels)``.
"""

from ._nodes import Node, get, node, nodes, schema_of

__all__ = ["Node", "get", "node", "nodes", "schema_of"]
