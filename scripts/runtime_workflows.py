"""R07 microscope operations against the real TLS runtime, using simulated devices."""
import hashlib
import json
import struct


def completed_job(client, reference, until):
    href = reference["href"]

    def finished():
        job = client.json(href)
        return job if job["state"] in ("completed", "failed", "cancelled") else None

    job = until(finished, "microscope job " + href, timeout=300)
    assert job["state"] == "completed", f"{href}: {job.get('error')}"
    return href, job


def record_id(job):
    return job["results"][0].split("/")[-1]


def check(client, frame, send_text, until, require_gpu):
    client.request("/api/v1/simulation/rate", method="PUT", value=10.0, expected=201)
    client.action("stage/home", {})
    point = {"x": 20_000.0, "y": 12_500.0}
    focus = client.action("simulation/in_focus", point)
    client.action("stage/move_to", {**point, "z": focus})
    client.request("/api/v1/camera/roi", method="PUT",
                   value={"x": 556, "y": 360, "width": 256, "height": 192}, expected=201)
    roi = client.json("/api/v1/camera/roi")
    client.request("/api/v1/preview/defaults", method="PUT",
                   value={"width": 160, "height": 120, "quality": 70, "max_fps": 5.0}, expected=201)
    client.action("camera/start_streaming", {})
    # Browser binary streams upgrade first, then authenticate their first message.
    # Missing/invalid credentials must close before any preview can be delivered.
    for message in ({"width": 160}, {"authorization": "Bearer invalid"}):
        denied = client.websocket(authenticated=False, path="/api/v1/preview/stream",
                                  first_message=True)
        try:
            send_text(denied, json.dumps(message))
            opcode, reason = frame(denied)
            assert opcode == 8 and len(reason) >= 2 and struct.unpack("!H", reason[:2])[0] == 1008
        finally:
            denied.close()
    client.websocket(origin="https://untrusted.invalid", path="/api/v1/preview/stream",
                     first_message=True)
    assert client.json("/api/v1/preview/gpu") is require_gpu
    stream = client.websocket(path="/api/v1/preview/stream", first_message=True)
    try:
        stream.settimeout(90)  # Allow the simulator's first rendered exposure.
        send_text(stream, json.dumps({"width": 160, "height": 120, "max_fps": 5.0, "ack": 0}))
        opcode, data = frame(stream)
        assert opcode == 2 and data[:4] == b"OSPV" and data[4:7] == b"\x01\x01\x01"
        assert len(data) >= 76, "short preview header"
        header, sequence = struct.unpack_from("<2I", data, 8)
        width, height, size = struct.unpack_from("<3I", data, 64)
        assert header >= 76 and 0 < width <= 160 and 0 < height <= 120
        assert len(data) == header + size and data[header:header + 2] == b"\xff\xd8"
        assert data[-2:] == b"\xff\xd9", "invalid preview JPEG framing"
        start, end = struct.unpack_from("<2Q", data, 24)
        assert start <= end

        def rendered():
            sessions = client.json("/api/v1/preview/sessions")
            return next((s for s in sessions if s["sent"] > 0 and s["gpu"] is require_gpu), None)

        until(rendered, "preview backend reported by an actual rendered session")
        send_text(stream, json.dumps({"ack": sequence + 1, "width": 160, "height": 120, "max_fps": 5.0}))
    finally:
        stream.close()

    jobs = []
    href, dark_job = completed_job(client, client.action("calibration/dark", {"frames": 2}), until)
    jobs.append(href)
    dark = client.json("/api/v1/calibration/active")["dark"]
    assert dark and dark["id"] == record_id(dark_job)

    project = client.action("projects/create", {"name": "R07 simulated microscope"})["id"]
    href, capture = completed_job(client, client.action("capture/single", {
        "project": project, "name": "runtime single", "rendition": "png",
    }), until)
    jobs.append(href)
    captured_id = record_id(capture)
    project_path = f"/api/v1/projects/project/{project}"
    captured = next(a for a in client.json(project_path)["acquisitions"] if a["id"] == captured_id)
    assert len(captured["frames"]) == 1
    assert (captured["image"]["width"], captured["image"]["height"]) == (roi["width"], roi["height"])

    request = {"pattern": "snake", "region": {"rect": {
        "origin": {"x": point["x"] - 150.0, "y": point["y"] - 100.0},
        "size": {"width": 300.0, "height": 200.0},
    }}}
    planned = client.action("scan/plan", {"request": request})
    tiles = planned["plan"]["tiles"]
    assert 1 < len(tiles) <= 9 and planned["estimate"]["frames"] == len(tiles)
    href, scanned = completed_job(client, client.action("scan/run", {
        "project": project, "request": request, "name": "runtime snake",
    }), until)
    jobs.append(href)
    scan_id = record_id(scanned)
    scan = next(a for a in client.json(project_path)["acquisitions"] if a["id"] == scan_id)
    assert len(scan["frames"]) == len(tiles) and scan["plan"]["pattern"] == "snake"
    for taken, tile in zip(scan["frames"], tiles):
        for axis in ("x", "y"):
            assert abs(taken["position"][axis] - tile[axis + "_um"]) < 2.5, "scan position outside source tolerance"
    client.action("camera/stop_streaming", {})

    href, stitched = completed_job(client, client.action("jobs/submit", {
        "kind": "stitch.run", "params": {"project": project, "acquisition": scan_id},
    }), until)
    jobs.append(href)
    mosaic_id = record_id(stitched)
    dzi_path = f"/api/v1/zoom/{project}/results/{mosaic_id}/image.dzi"
    dzi = client.request(dzi_path)
    assert b"<Image" in dzi and b"<Size" in dzi
    client.request(dzi_path, authenticated=False, expected=401)

    href, reported = completed_job(client, client.action("jobs/submit", {
        "kind": "report.make", "params": {"project": project, "options": {}},
    }), until)
    jobs.append(href)
    report_id = record_id(reported)
    base = f"/api/v1/data/{project}/reports/{report_id}"
    files = {}
    for suffix in ("report.pdf", "report.html", "model.json"):
        url = f"{base}/{suffix}"
        content = client.request(url)
        assert content
        if suffix.endswith(".pdf"):
            assert content.startswith(b"%PDF-")
        if suffix.endswith(".json"):
            assert json.loads(content)["sections"]
        files[url] = hashlib.sha256(content).hexdigest()
    return {
        "project_path": project_path, "jobs": jobs, "files": files,
        "dark": dark["id"], "scan": scan_id, "frames": len(tiles),
        "mosaic": mosaic_id, "preview_gpu": require_gpu,
    }


def verify(client, evidence):
    acquisitions = client.json(evidence["project_path"])["acquisitions"]
    scan = next(a for a in acquisitions if a["id"] == evidence["scan"])
    assert len(scan["frames"]) == evidence["frames"]
    assert client.json("/api/v1/calibration/active")["dark"]["id"] == evidence["dark"]
    for href in evidence["jobs"]:
        assert client.json(href)["state"] == "completed"
    for url, digest in evidence["files"].items():
        assert hashlib.sha256(client.request(url)).hexdigest() == digest, "scientific output changed across restart"
