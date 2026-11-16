"""A sample report generator for OScope (P47): a camera's field of view and
pixel size at the sample under each objective, as a table, a chart of the
fields' widths, and a sketch of the fields about one centre, drawn as a
PNG."""

import base64
import struct
import zlib

from oscope_ext import driver

SCHEMA = {
    "type": "object",
    "properties": {
        "sensor": {
            "type": "object",
            "description": "The sensor: its pixels across and down, and their pitch in µm.",
            "properties": {
                "width": {"type": "integer", "minimum": 1},
                "height": {"type": "integer", "minimum": 1},
                "pitch_um": {"type": "number", "exclusiveMinimum": 0},
            },
            "required": ["width", "height", "pitch_um"],
        },
        "magnifications": {
            "type": "array",
            "items": {"type": "number", "exclusiveMinimum": 0},
            "minItems": 1,
            "maxItems": 12,
            "description": "The objectives' magnifications, with their tube lens.",
        },
    },
    "required": ["sensor", "magnifications"],
}


def png(width, height, rows):
    """A PNG of 8-bit grey `rows`, as the PNG specification lays it out: a
    signature, then the header, data and end chunks, each with its CRC."""

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    header = struct.pack(">IIBBBBB", width, height, 8, 0, 0, 0, 0)
    data = zlib.compress(b"".join(b"\0" + bytes(row) for row in rows))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", data) + chunk(b"IEND", b"")


def sketch(fields, width=240):
    """The fields about one centre, each a rectangle's outline, to scale:
    the widest across the whole picture."""
    scale = (width - 1) / max(w for w, _ in fields)
    height = round(max(h for _, h in fields) * scale) + 1
    rows = [[255] * width for _ in range(height)]
    for w, h in fields:
        left, right = round((width - 1 - w * scale) / 2), round((width - 1 + w * scale) / 2)
        top, bottom = round((height - 1 - h * scale) / 2), round((height - 1 + h * scale) / 2)
        for x in range(left, right + 1):
            rows[top][x] = rows[bottom][x] = 0
        for y in range(top, bottom + 1):
            rows[y][left] = rows[y][right] = 0
    return png(width, height, rows)


@driver("example.fields", kind="report")
class Fields:
    def schema(self):
        return SCHEMA

    def generate(self, params):
        try:
            sensor = params["sensor"]
            across, down = int(sensor["width"]), int(sensor["height"])
            pitch = float(sensor["pitch_um"])
            magnifications = [float(m) for m in params["magnifications"]]
        except (KeyError, TypeError, ValueError) as error:
            raise ValueError(f"fields need a sensor and magnifications: {error}") from None
        if min(across, down, pitch, *magnifications) <= 0 or not 1 <= len(magnifications) <= 12:
            raise ValueError("fields need a sensor's size and pitch, and 1 to 12 magnifications, above 0")
        # The sensor's size over the magnification, in mm.
        fields = [(across * pitch / m / 1000, down * pitch / m / 1000) for m in magnifications]
        names = [f"{m:g}×" for m in magnifications]
        rows = [
            [name, f"{w:.3f} × {h:.3f}", f"{pitch / m:.3f}"]
            for name, m, (w, h) in zip(names, magnifications, fields)
        ]
        size = f"{across} × {down} pixels of {pitch:g} µm"
        blocks = [
            {"kind": "text", "text": f"A sensor of {size}: {across * pitch / 1000:.2f} × {down * pitch / 1000:.2f} mm."},
            {"kind": "table", "caption": "Each objective's field at the sample",
             "columns": ["Objective", "Field (mm)", "Pixel (µm)"], "rows": rows},
            {"kind": "chart", "caption": "The fields' widths", "chart": {
                "kind": "bars", "labels": names, "values": [w for w, _ in fields],
                "x_label": "objective", "y_label": "width (mm)"}},
            {"kind": "image", "name": "fields.png", "caption": "The fields about one centre, to scale",
             "width": 0.5},
        ]
        return {
            "title": "Fields of view",
            "date": "",
            "metadata": [{"name": "Sensor", "value": size}],
            "sections": [{"heading": "The objectives", "blocks": blocks}],
            "provenance": [{"name": "Generator", "value": "example.fields 0.1.0"}],
            "images": {"fields.png": base64.b64encode(sketch(fields)).decode("ascii")},
        }
