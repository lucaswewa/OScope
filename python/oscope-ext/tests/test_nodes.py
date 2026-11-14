"""Declaring nodes: schemas from type hints, explicit schemas, descriptors as
the pipeline reads them, and outputs."""

from typing import Literal

import numpy as np
import pytest

import oscope_ext
from oscope_ext import node, schema_of


def test_a_schema_comes_from_type_hints_and_defaults():
    def f(
        image,
        *,
        sigma: float = 1.5,
        size: int,
        mode: Literal["reflect", "nearest"] = "reflect",
        keep: bool = False,
        label: str = "x",
    ): ...

    assert schema_of(f) == {
        "type": "object",
        "properties": {
            "sigma": {"type": "number", "default": 1.5},
            "size": {"type": "integer"},
            "mode": {"enum": ["reflect", "nearest"], "default": "reflect"},
            "keep": {"type": "boolean", "default": False},
            "label": {"type": "string", "default": "x"},
        },
        "additionalProperties": False,
        "required": ["size"],
    }


def test_a_hint_without_a_schema_is_refused():
    def f(image, *, weights: list[float] = []): ...

    with pytest.raises(TypeError, match=r"f\(weights\)"):
        schema_of(f)


def test_a_declared_node_describes_itself_as_the_pipeline_does():
    @node("test.blend", category="test", outputs=("mixed",), live=True, tileable=2)
    def blend(a, b, *, k: float = 0.5):
        """Mixes two images.

        By ``k``."""
        return a * (1 - k) + b * k

    declared = oscope_ext.get("test.blend")
    assert declared.descriptor() == {
        "id": "test.blend",
        "version": 1,
        "category": "test",
        "description": "Mixes two images.",
        "inputs": [{"name": "a", "type": "image"}, {"name": "b", "type": "image"}],
        "outputs": [{"name": "mixed", "type": "image"}],
        "params": {
            "type": "object",
            "properties": {"k": {"type": "number", "default": 0.5}},
            "additionalProperties": False,
        },
        "capabilities": {"cpu": True, "cuda": False, "live": True, "tileable": True, "halo": 2},
    }
    a, b = np.zeros((2, 3, 1), np.float32), np.ones((2, 3, 1), np.float32)
    [mixed] = declared.run([a, b], {"k": 0.25})
    assert mixed.dtype == np.float32 and mixed.shape == (2, 3, 1)
    assert np.all(mixed == 0.25)


def test_outputs_become_float32_images_and_their_count_is_checked():
    @node("test.grey", outputs=("grey", "first"), params={"type": "object"})
    def grey(image):
        return image.mean(axis=2), image[:, :, :1].astype(np.float64)

    image = np.arange(12, dtype=np.float32).reshape(2, 2, 3)
    declared = oscope_ext.get("test.grey")
    grey_image, first = declared.run([image], {})
    assert grey_image.shape == (2, 2, 1) and first.dtype == np.float32
    assert grey_image[0, 0, 0] == 1.0
    assert declared.params == {"type": "object"}

    @node("test.short", outputs=("a", "b"))
    def short(image):
        return image

    with pytest.raises(ValueError, match="gave 1 outputs, and has 2"):
        oscope_ext.get("test.short").run([image], {})


def test_a_node_is_declared_once():
    @node("test.once")
    def once(image):
        return image

    with pytest.raises(ValueError, match="declared twice"):
        node("test.once")(once)
