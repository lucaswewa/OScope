"""A sample scan workflow for OScope (P36): tiles in a square spiral out
from a centre, a ring at a time, so that a scan starts where the user is
looking and grows from there."""

from oscope_ext import driver

SCHEMA = {
    "type": "object",
    "properties": {
        "x_um": {"type": "number", "description": "The centre's x, in µm."},
        "y_um": {"type": "number", "description": "The centre's y, in µm."},
        "rings": {
            "type": "integer",
            "minimum": 0,
            "maximum": 50,
            "description": "How many rings of tiles about the centre's.",
        },
    },
    "required": ["x_um", "y_um", "rings"],
}


@driver("example.spiral", kind="workflow")
class Spiral:
    def schema(self):
        return SCHEMA

    def plan(self, request):
        params, field = request["params"], request["field"]
        try:
            x, y, rings = float(params["x_um"]), float(params["y_um"]), int(params["rings"])
        except (KeyError, TypeError, ValueError) as error:
            raise ValueError(f"a spiral needs x_um, y_um and rings: {error}") from None
        if not 0 <= rings <= 50:
            raise ValueError(f"{rings} rings: from 0 to 50")
        dx, dy = field["step_um"]["width"], field["step_um"]["height"]
        tiles = [(0, 0)]
        for ring in range(1, rings + 1):
            # Round the ring from its top-left corner, clockwise.
            side = [(i, -ring) for i in range(-ring, ring)]
            side += [(ring, j) for j in range(-ring, ring)]
            side += [(-i, ring) for i in range(-ring, ring)]
            side += [(-ring, -j) for j in range(-ring, ring)]
            tiles += side
        return [
            {"x_um": x + i * dx, "y_um": y + j * dy, "name": f"ring {max(abs(i), abs(j))}"}
            for i, j in tiles
        ]
