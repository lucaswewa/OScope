# ADR-0051: Uploads

- Status: Accepted
- Date: 2026-11-21
- Phase: W6.3

## Context

- **The app's files come from its user's computer:** a TIFF or a stack to import into a project (W6.3), an image to load as the simulator's sample (W8.x), and, later, whatever a node reads.
- **The server's import reads a file on its own disk** (P30, ADR-0037): the `projects.import` job's `path`. A browser can't name one.
- **P24 has an upload route of its own,** `POST /api/v1/extensions/install`, which takes a package and installs it at once. P30 left the general route to Milestone 2. The plan reserves `POST /api/v1/upload/…` on the media plane (§5.4).
- **The scratch space is the disk for what is passing through** (ADR-0022): its free space is watched, it keeps a reserve, and its `tmp/` is emptied when it opens.
- **Every request that changes state needs the lease** (ADR-0031), the media plane's routes among them: teta-wot's authoriser sees a merged route's `POST` as a change.
- **A browser tells an upload's progress only through `XMLHttpRequest`** (W1.3's `send`), which sends the file as the request's body, not as a form.

## Decision

**1. The route.** `POST /api/v1/upload/{name}`, on the media plane:
- The body is the file's bytes, as they are: no form, no encoding.
- `{name}` is the file's name, percent-encoded. An importer chooses by it, so it is kept.
- It answers 201 with the file staged: its `id`, `name`, `bytes`, and when it `expires`.
- It needs the lease, as every request that changes state does.

**2. What it refuses.**
- **A name that isn't one plain part of a path,** with 400:
  - one with a separator, `/` or `\`, or a `:`;
  - one that starts with a dot, so `.` and `..` among them;
  - one that ends with a dot or a space;
  - one with a control character, or a character Windows forbids in a name;
  - a name Windows keeps for a device, such as `CON` or `com1.png`, on both platforms;
  - one over 255 bytes.
- **A file over the limit,** with 413: 16 GiB by default, some hundred of the camera's full frames. A length said before the file came is refused at once; the bytes are counted as they come, so a length left out, or understated, is refused too.
- **A file the scratch space has no room for,** with 507: one that would leave less than its reserve free.

**3. Where it is staged.**
- It is written in the scratch space's `tmp/`, then moved into `uploads/<id>/<name>` once it has all come.
- An upload cut short, by its user or a connection lost, leaves nothing: its part in `tmp/` is removed as the request ends, and `tmp/` is emptied at each start.
- The ID is the server's, a UUID: a job names a staged file by it, never by a path.

**4. How long it stays.**
- **Once used, it goes:** the import job removes the file it imported.
- **Unused, it goes after a day:** the route sweeps what is older at each upload, at each start, and every hour.
- A failed import leaves its file for another try within the day.

**5. The jobs read staged files by their ID.** `projects.import` takes `upload`, a staged file's ID, or `path`, a file on the server, one of them.

## Consequences

- **Uploading and using are two requests:** the upload, with its progress; then the job, with its own. A client that stops between them leaves a file for a day.
- **The lease's holder alone uploads.** A viewer can't fill the scratch disk.
- **Every phase that takes a file takes it the same way:** the simulator's samples (W8.x), and later the nodes', name a staged file's ID.
- **The desktop app's drops** come to the page once its window lets them (W9.1); the route is the same.
- **One request a file:** a client that wants a large file resumed sends it again. A resumable protocol, such as tus, can come later behind the same IDs.

## Alternatives considered

- **`multipart/form-data`:** the browser's own form, several files at once. Its boundaries must be parsed as the bytes stream, and `XMLHttpRequest` tells progress as well without it. One file a request is simpler.
- **The upload as the job's input:** one request, but a job's parameters are JSON, and a large file in one is out of the question.
- **Staging in the data root:** beside the projects, but the data root is for what is kept. The scratch space is for what passes, with its reserve watched.
- **A resumable upload (tus):** a protocol and a dependency more, for files the network carries in minutes. Left for when it is needed.
- **Keeping staged files until their user deletes them:** they would fill the scratch disk.
