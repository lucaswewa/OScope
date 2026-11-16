# ext-python-cells

A stub Python extension (P46): where a learned cell segmenter plugs into OScope, as a pipeline node.

- **The node,** `example.cells_segment`, takes an image and gives nuclei as labels, an image of object numbers like `cells.split`'s.
- **With [cellpose](https://github.com/MouseLand/cellpose) or [StarDist](https://github.com/stardist/stardist)** installed in the extension's environment, it calls that library through its documented interface.
- **With neither,** it gives a stand-in: a mask of the pixels above the image's mean, so that a recipe using it runs end to end.

Neither library is a dependency of the stub, and neither is distributed with OScope: each comes under its own licence, which the user accepts by installing it.

## How to try it

With uv installed, the extension tests pack, install and run it, on the stand-in: `OSCOPE_PYTHON=1 cargo test -p oscope-ext --test python cells`. Pack it with `cargo run -p oscope-server -- ext pack examples/ext-python-cells`, and install the package into a server's extensions as any other; the node then appears in its pipeline's catalogue. To use cellpose, install it into the extension's environment, `<extensions>/org.oscope.examples.cells-stub/0.1.0/.oscope/venv`, with uv.

## Where to go from here

A real extension would:

- give the learned model's labels to `cells.measure`, OScope's own node, for each object's measures and outline;
- pick the model and its diameter as parameters;
- keep the model loaded between calls, as the worker stays up.
