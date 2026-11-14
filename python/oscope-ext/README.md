# oscope-ext (Python)

The SDK of OScope's Python extensions (P26, [ADR-0020](../../docs/adr/0020-the-python-extension-runtime.md)): the `oscope_ext` package.

## Writing nodes

An extension is an `.omext` package of `kind = "python"` ([ADR-0018](../../docs/adr/0018-the-omext-package-format.md)). Its manifest names its dependencies and its entry, a module that declares its nodes:

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

A module may drive a camera, a stage or lights too, with `@driver` (P27, [ADR-0036](../../docs/adr/0036-extension-drivers-and-panels.md)). The manifest lists each driver's name in `provides.drivers`, and a server's configuration names it in `[devices]`.

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

## How OScope runs them

When OScope installs the package, uv makes its environment: a Python its `requires-python` allows, its dependencies, and this SDK. Then OScope starts a worker, `python -m oscope_ext.worker <entry>`, on the first run of one of its nodes, and keeps it warm. Messages travel on stdin and stdout, so a node's `print` goes to OScope's log.

## Developing the SDK

```sh
cd python/oscope-ext
uv run --locked pytest
```

`pyproject.toml` holds packages to those two weeks old when P26 began (`exclude-newer`), and `uv.lock` pins them.
