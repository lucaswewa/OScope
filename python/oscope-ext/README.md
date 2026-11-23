# oscope-ext (Python)

This port runs workers in the Linux backend container, using its Python 3.12
and Linux shared-memory files. Interpreter downloads are disabled. See the
[R05 manual procedure](../../docs/phases/r05-storage-processing.md#manual-test)
for the required SDK/worker gate.

The SDK of OScope's Python extensions (P26, [ADR-0020](https://github.com/lucaswewa/OScope/blob/44bc08a53f98dc29db1fa6b488a4f6269732a71a/docs/adr/0020-the-python-extension-runtime.md)): the `oscope_ext` package.

## Writing nodes

An extension is an `.omext` package of `kind = "python"` ([ADR-0018](https://github.com/lucaswewa/OScope/blob/44bc08a53f98dc29db1fa6b488a4f6269732a71a/docs/adr/0018-the-omext-package-format.md)). Its manifest names its dependencies and its entry, a module that declares its nodes:

```python
import numpy as np
from oscope_ext import node

@node("example.invert", category="example")
def invert(image: np.ndarray, *, level: float = 1.0) -> np.ndarray:
    """Turns each sample into `level` less it."""
    return level - image
```

- **Inputs** are the function's positional parameters: images, as NumPy arrays of `float32` shaped `(height, width, channels)`. They share the frames' memory with OScope, read-only.
- **Parameters** are its keyword-only parameters. Their JSON Schema comes from their type hints (`int`, `float`, `bool`, `str`, or a `Literal`) and defaults, unless `params=` gives one. OScope checks parameters against it before a run.
- **Outputs:** it returns an image, `(height, width)` or `(height, width, channels)`, or a tuple of them, one for each name in `outputs=` (`("image",)` by default).
- **`live=True`** says it is quick enough for every frame of the live view. **`tileable=n`** says it can run tile by tile, reading `n` pixels around a tile.

## Writing drivers

A module may drive a camera, a stage or lights too, with `@driver` (P27, [ADR-0036](https://github.com/lucaswewa/OScope/blob/44bc08a53f98dc29db1fa6b488a4f6269732a71a/docs/adr/0036-extension-drivers-and-panels.md)). The manifest lists each driver's name in `provides.drivers`, and a server's configuration names it in `[devices]`.

```python
from oscope_ext import driver

@driver("example.stage", kind="stage")
class Stage:
    def capabilities(self):
        travel = {"min": 0.0, "max": 100000.0}          # µm
        return {"model": "example stage", "x": travel, "y": travel, "z": travel,
                "max_speed": 50000.0, "default_speed": 10000.0, "homing": False,
                "encoders": False, "trigger_output": False, "report_interval": 10.0}
    def status(self): return {"state": "idle"}           # moving, homing, hold, alarm
    def position(self): return {"x": 0.0, "y": 0.0, "z": 0.0}
    def move_to(self, request): ...                      # request["target"], request["speed"]
    def jog(self, request): ...                          # request["axis"], ["towards_max"], ["speed"]
    def stop(self): ...
```

- OScope makes one instance, with no arguments, when it first needs it. It calls the methods with the JSON of the HAL's types: no argument when there is nothing to pass, or one.
- A **camera** has `capabilities()`, `configure(settings)`, `start(settings)`, `capture()` and `stop()`. `capture` returns the next frame as an array laid out as the settings say, `uint8` or `uint16`, which reaches OScope through shared memory. **Lights** have `capabilities()`, `state()` and `set(light)`.
- OScope does what its HAL promises around them: it checks settings, targets, speeds and lights against the capabilities, waits for moves to end, keeps the track of positions, and times and numbers frames. So a driver only applies, starts, reports and captures, and passes the HAL's conformance suite.

## Writing importers and exporters

A module may bring other file formats in and out, with `@driver(..., kind="importer")` and `kind="exporter"` (P30, [ADR-0037](https://github.com/lucaswewa/OScope/blob/44bc08a53f98dc29db1fa6b488a4f6269732a71a/docs/adr/0037-importers-exporters-and-the-abi-s-third-version.md)). The manifest lists them in `provides.importers` and `provides.exporters`; their names are their formats' IDs.

```python
@driver("example.npy.read", kind="importer")
class NpyReader:
    def format(self): return {"name": "NumPy array", "extensions": ["npy"]}
    def probe(self, request):                            # request["path"]
        array = np.load(request["path"])
        return {"width": array.shape[1], "height": array.shape[0], "channels": 1,
                "planes": 1, "sample": "u16"}            # u8, u16 or f32
    def read(self, request):                             # request["path"], ["plane"]
        return np.load(request["path"])                  # the plane's samples
```

- An **importer** has `format()`, `probe({"path"})`, which says what the file holds, and `read({"path", "plane"})`, which returns a plane as an array, OScope's through shared memory. A probe may add `pixel_size` (`{"x", "y", "z"}` in µm) and `channel_names`.
- An **exporter** has `format()`, `begin({"path", "image", "name"})`, then `write({"plane"}, pixels)` for each plane, its samples' bytes, little-endian, in a flat array, and `end()`. OScope gives it one image at a time.
- The sample [`ext-python-node`](../../examples/ext-python-node/README.md) has both, for `.npy`.

## How OScope runs them

When OScope installs the package, uv makes its environment: a Python its `requires-python` allows, its dependencies, and this SDK. Then OScope starts a worker, `python -m oscope_ext.worker <entry>`, on the first run of one of its nodes, and keeps it warm. Messages travel on stdin and stdout, so a node's `print` goes to OScope's log.

## Developing the SDK

```sh
cd python/oscope-ext
uv run --locked pytest
```

`pyproject.toml` holds packages to those two weeks old when P26 began (`exclude-newer`), and `uv.lock` pins them.
