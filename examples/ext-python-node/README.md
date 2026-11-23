# ext-python-node

A sample Python extension for OScope (P26, [ADR-0020](https://github.com/lucaswewa/OScope/blob/44bc08a53f98dc29db1fa6b488a4f6269732a71a/docs/adr/0020-the-python-extension-runtime.md)): one node, `example.denoise_tv`, which removes noise but keeps edges, with scikit-image's total-variation denoising. It is what a third party writes: a manifest and a module, using the [Python SDK](../../python/oscope-ext/README.md).

It isn't a crate, so the Cargo workspace leaves it out. [`ex-python-node`](../ex-python-node/README.md) installs it and runs it in a recipe.

## The node

[`tv_denoise/__init__.py`](tv_denoise/__init__.py):

```python
@node("example.denoise_tv", category="example", live=True)
def denoise_tv(image: np.ndarray, *, weight: float = 0.1, iterations: int = 200) -> np.ndarray:
    """Removes noise but keeps edges: Chambolle's total-variation denoising."""
    return denoise_tv_chambolle(image, weight=weight, max_num_iter=iterations, channel_axis=-1)
```

- **Input** `image`, **output** `image`: NumPy arrays shaped `(height, width, channels)`, the input shared with OScope through shared memory.
- **Parameters** from the type hints: `weight`, a number, 0.1 by default (more smooths more), and `iterations`, an integer, 200 by default.
- **Live,** so that live recipes may use it at low rates: a live recipe is checked against its time budget before it runs.

## The importer and exporter

The same module brings in and takes out NumPy's `.npy` files (P30, [ADR-0037](https://github.com/lucaswewa/OScope/blob/44bc08a53f98dc29db1fa6b488a4f6269732a71a/docs/adr/0037-importers-exporters-and-the-abi-s-third-version.md)), with `@driver(..., kind="importer")` and `kind="exporter"`:

- **`example.npy.read`** says what a file holds (`probe`), then gives each plane as an array (`read`). An array of `(height, width)`, `(height, width, channels)` or `(planes, height, width, channels)` becomes an acquisition, a plane each.
- **`example.npy.write`** takes an image's description (`begin`), each plane's bytes in turn (`write`), and saves the stack (`end`).

The `projects.import` and `projects.export` jobs use them for files ending in `.npy`, or when asked for them by name.

## The manifest

[`manifest.toml`](manifest.toml) names its entry module, the Pythons it allows (`>=3.12,<3.15`) and its dependency, `scikit-image`. When OScope installs the package, uv makes its environment: a uv-managed Python, scikit-image with what it needs, and OScope's SDK, all at least two weeks old.

Pack it with `cargo run -p oscope-server -- ext pack examples/ext-python-node`.
