"""A stub Python extension for OScope (P46): where a learned cell
segmenter plugs in. With cellpose or StarDist installed in the
extension's environment, the node calls it. Without either, it gives a
stand-in, a mask of the pixels above the image's mean, so that a recipe
runs end to end."""

import numpy as np
from oscope_ext import node


def _grey(image: np.ndarray) -> np.ndarray:
    return image.mean(axis=-1) if image.ndim == 3 else image


def _learned(grey: np.ndarray):
    """Labels from cellpose or StarDist, whichever is installed: None if
    neither is. Each is called through its own documented interface."""
    try:
        from cellpose import models
    except ImportError:
        pass
    else:
        masks = models.CellposeModel(gpu=False).eval(grey)[0]
        return masks.astype(np.float32)
    try:
        from stardist.models import StarDist2D
    except ImportError:
        return None
    scaled = (grey - grey.min()) / max(float(np.ptp(grey)), 1e-12)
    labels = StarDist2D.from_pretrained("2D_versatile_fluo").predict_instances(scaled)[0]
    return labels.astype(np.float32)


@node("example.cells_segment", category="cells")
def segment(image: np.ndarray) -> np.ndarray:
    """Nuclei as labels, by cellpose or StarDist when installed; or a
    stand-in, a mask of the pixels above the image's mean."""
    grey = _grey(image)
    labels = _learned(grey)
    if labels is None:
        labels = (grey > grey.mean()).astype(np.float32)
    return labels[..., None]
