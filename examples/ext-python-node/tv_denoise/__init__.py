"""A sample Python extension for OScope (P26): scikit-image's total-variation
denoising, as a node."""

import numpy as np
from oscope_ext import node
from skimage.restoration import denoise_tv_chambolle


# Live too, at low rates: a live recipe's budget decides (P27).
@node("example.denoise_tv", category="example", live=True)
def denoise_tv(image: np.ndarray, *, weight: float = 0.1, iterations: int = 200) -> np.ndarray:
    """Removes noise but keeps edges: Chambolle's total-variation denoising."""
    return denoise_tv_chambolle(image, weight=weight, max_num_iter=iterations, channel_axis=-1)
