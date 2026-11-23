"""Nodes: the decorator that declares them, their parameters' schemas, and
running them."""

from __future__ import annotations

import inspect
import typing
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any, Literal

import numpy as np

_NODES: dict[str, Node] = {}


@dataclass(frozen=True)
class Node:
    """A node, as an extension declares it with :func:`node`."""

    id: str
    version: int
    category: str
    description: str
    inputs: tuple[str, ...]
    outputs: tuple[str, ...]
    params: dict[str, Any]
    live: bool
    tileable: int | None
    function: Callable[..., Any]

    def descriptor(self) -> dict[str, Any]:
        """The node as OScope's pipeline describes it (ADR-0017)."""

        def port(name: str) -> dict[str, str]:
            return {"name": name, "type": "image"}

        return {
            "id": self.id,
            "version": self.version,
            "category": self.category,
            "description": self.description,
            "inputs": [port(name) for name in self.inputs],
            "outputs": [port(name) for name in self.outputs],
            "params": self.params,
            "capabilities": {
                "cpu": True,
                "cuda": False,
                "live": self.live,
                "tileable": self.tileable is not None,
                "halo": self.tileable or 0,
            },
        }

    def run(self, inputs: list[np.ndarray], params: dict[str, Any]) -> list[np.ndarray]:
        """Runs the node: its outputs, each a C-ordered float32 array of
        shape ``(height, width, channels)``."""
        result = self.function(*inputs, **params)
        results = result if isinstance(result, tuple) else (result,)
        if len(results) != len(self.outputs):
            raise ValueError(f"{self.id} gave {len(results)} outputs, and has {len(self.outputs)}")
        return [_image(result, name) for result, name in zip(results, self.outputs)]


def node(
    id: str,
    *,
    version: int = 1,
    category: str = "python",
    description: str | None = None,
    outputs: tuple[str, ...] = ("image",),
    params: dict[str, Any] | None = None,
    live: bool = False,
    tileable: int | None = None,
) -> Callable[[Callable[..., Any]], Callable[..., Any]]:
    """Declares a function as a node, ``id``.

    - Its positional parameters are its inputs: images, as NumPy arrays of
      float32, shaped ``(height, width, channels)``.
    - Its keyword-only parameters are its parameters. Their JSON Schema comes
      from their type hints and defaults, unless ``params`` gives it.
    - It returns its output, or a tuple of its ``outputs``: arrays shaped
      ``(height, width)`` or ``(height, width, channels)``.
    - Its description is its docstring's first line, unless given.
    - ``live``: quick enough for every frame of the live view.
      ``tileable``: it can run tile by tile, reading this many pixels around
      a tile.
    """

    def declare(function: Callable[..., Any]) -> Callable[..., Any]:
        if id in _NODES:
            raise ValueError(f"the node {id} is declared twice")
        positional = (inspect.Parameter.POSITIONAL_ONLY, inspect.Parameter.POSITIONAL_OR_KEYWORD)
        parameters = inspect.signature(function).parameters.items()
        inputs = tuple(name for name, p in parameters if p.kind in positional)
        schema = params if params is not None else schema_of(function)
        doc = inspect.getdoc(function) or ""
        text = description if description is not None else doc.split("\n")[0]
        declared = Node(id, version, category, text, inputs, tuple(outputs), schema, live, tileable, function)
        _NODES[id] = declared
        return function

    return declare


def nodes() -> list[Node]:
    """The nodes declared so far, in the order they were."""
    return list(_NODES.values())


def get(id: str) -> Node:
    """The node ``id``."""
    if id not in _NODES:
        raise KeyError(f"no node {id} is declared")
    return _NODES[id]


def schema_of(function: Callable[..., Any]) -> dict[str, Any]:
    """The JSON Schema of a function's keyword-only parameters, from their
    type hints (``int``, ``float``, ``bool``, ``str`` or a ``Literal``) and
    their defaults: a parameter without a default is required."""
    hints = typing.get_type_hints(function)
    properties: dict[str, Any] = {}
    required = []
    for name, parameter in inspect.signature(function).parameters.items():
        if parameter.kind is not inspect.Parameter.KEYWORD_ONLY:
            continue
        schema = _schema(hints.get(name), f"{function.__name__}({name})")
        if parameter.default is inspect.Parameter.empty:
            required.append(name)
        else:
            schema["default"] = parameter.default
        properties[name] = schema
    schema = {"type": "object", "properties": properties, "additionalProperties": False}
    if required:
        schema["required"] = required
    return schema


def _schema(hint: Any, where: str) -> dict[str, Any]:
    for kind, name in ((bool, "boolean"), (int, "integer"), (float, "number"), (str, "string")):
        if hint is kind:
            return {"type": name}
    if typing.get_origin(hint) is Literal:
        return {"enum": list(typing.get_args(hint))}
    raise TypeError(f"{where}: its type hint isn't int, float, bool, str or a Literal: give the node's params")


def _image(array: Any, name: str) -> np.ndarray:
    image = np.asarray(array, dtype=np.float32)
    if image.ndim == 2:
        image = image[:, :, np.newaxis]
    if image.ndim != 3:
        raise ValueError(f"output {name}: an image is (height, width) or (height, width, channels), not {image.shape}")
    return np.ascontiguousarray(image)
