"""A sample Python extension for OScope (P26): scikit-image's total-variation
denoising, as a node; and an importer and an exporter of NumPy's ``.npy``
files (P30)."""

import numpy as np
from oscope_ext import driver, node
from skimage.restoration import denoise_tv_chambolle


# Live too, at low rates: a live recipe's budget decides (P27).
@node("example.denoise_tv", category="example", live=True)
def denoise_tv(image: np.ndarray, *, weight: float = 0.1, iterations: int = 200) -> np.ndarray:
    """Removes noise but keeps edges: Chambolle's total-variation denoising."""
    return denoise_tv_chambolle(image, weight=weight, max_num_iter=iterations, channel_axis=-1)


SAMPLES = {"uint8": "u8", "uint16": "u16", "float32": "f32"}
DTYPES = {"u8": "<u1", "u16": "<u2", "f32": "<f4"}


def npy_format():
    return {"name": "NumPy array", "extensions": ["npy"]}


@driver("example.npy.read", kind="importer")
class NpyReader:
    """NumPy's ``.npy``: an array of ``(height, width)``, ``(height, width,
    channels)`` or ``(planes, height, width, channels)``, of 8 or 16 bits or
    32-bit floats."""

    format = staticmethod(npy_format)

    @staticmethod
    def _array(path):
        array = np.load(path, mmap_mode="r")
        if array.ndim == 2:
            return array[None, :, :, None]
        return array[None] if array.ndim == 3 else array

    def probe(self, params):
        array = self._array(params["path"])
        if array.ndim != 4 or array.dtype.name not in SAMPLES:
            raise ValueError(f"an array of {array.dtype} shaped {array.shape}")
        planes, height, width, channels = array.shape
        return {"width": width, "height": height, "channels": channels, "planes": planes,
                "sample": SAMPLES[array.dtype.name]}

    def read(self, params):
        return np.asarray(self._array(params["path"])[params["plane"]])


@driver("example.npy.write", kind="exporter")
class NpyWriter:
    """NumPy's ``.npy``: an array of ``(planes, height, width, channels)``."""

    format = staticmethod(npy_format)

    def begin(self, params):
        self.path, self.image, self.planes = params["path"], params["image"], []

    def write(self, params, pixels):
        image = self.image
        plane = pixels.view(DTYPES[image["sample"]])
        self.planes.append(plane.reshape(image["height"], image["width"], image["channels"]))

    def end(self):
        with open(self.path, "wb") as out:
            np.save(out, np.stack(self.planes))
