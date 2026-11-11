# ADR-0002: Clean room: what OScope may take from `microscope`, and the dependency licence policy

- Status: Accepted
- Date: 2026-11-11
- Phase: P00

## Context

The owner's earlier project, `microscope`, overlaps heavily with OScope. It is also MIT-licensed and built on `teta-wot`, and includes a simulator, autofocus, scans, stitching, a gallery and a Vue app.

- **OScope's requirements differ at the root:** 16-bit raw frames, a GPU processing pipeline, third-party extensions, and 3D. While planning, the owner chose a **clean room** (D1): OScope is written fresh, taking ideas and look but no code.
- **Third-party material needs rules too:**
  - Tucsen's sample images, which goal.md points to, are Tucsen's copyrighted pictures.
  - The Tucsen SDK's redistribution terms are not yet known.
  - NVIDIA's CUDA libraries have their own EULA.
  - JavaScript libraries will be vendored into examples.
- **OScope is distributed under MIT,** so everything it depends on or ships must allow that.

## Decision

**1. Code is written fresh.**

- Nothing is copied from `microscope`, or from any other codebase, and nothing is ported line by line: no code, tests, styles or text.
- These may be reused, described in our own words:
  - **ideas:** hash routing, a host adapter for platform services, a sidecar process contract, generated API types, server-described UI panels;
  - **look:** the palette and magenta accent, compact density, the rail → control pane → view layout;
  - **lessons:** from its retrospective.

**2. Algorithms come from published methods.**

- Each spec-first phase cites its sources in its notes: phase correlation, demosaicing, Richardson–Lucy, colour deconvolution, focus measures, watershed, and others.

**3. Third-party material.**

- **Tucsen's sample images** are never committed. The simulator generates its own samples in their style.
- **The Tucsen SDK** is never redistributed unless its licence allows it; until that is confirmed (P02b), users install it.
- **CUDA runtime libraries** are shipped only as NVIDIA's EULA allows; P07 records which.
- **Vendored files** (JavaScript libraries for examples, for instance) keep their licence file beside them.

**4. Dependency licences.**

- Dependencies must allow MIT distribution: MIT, Apache-2.0, BSD, ISC, Zlib, Unicode, or public domain.
- MPL-2.0 is allowed only as an unmodified development dependency.
- A licence not on this list needs the owner's decision before it is added.
- `cargo-deny` enforces the list for crates from P01. The frontend's packages are checked when they are added (M2).

**5. Dependency age.**

- Only versions at least two weeks old are added (`npm install --before=…`; for crates, the release date is checked in the phase).

**6. Every pull request** ticks a clean-room checkbox ([`.github/pull_request_template.md`](../../.github/pull_request_template.md)).

This is an engineering policy, not legal advice.

## Consequences

- **Gains:**
  - OScope's provenance is simple: its own code, its listed dependencies, and cited methods.
  - Its core is designed for 16-bit frames, the GPU and extensions from the start, rather than retrofitted.
- **Costs:**
  - Capabilities `microscope` already has (a simulator, stitching, a gallery) are rebuilt, at the cost of phases.
  - The two-week rule occasionally holds back a fix that has only just been released.

## Alternatives considered

- **Fork `microscope`:** the fastest start, but OScope would inherit its 8-bit frame model and its naming, which would need reworking.
- **Shared crates for both products:** the least duplication, but it couples two products' release cycles.
- **Port `microscope`'s proven modules selectively:** a middle path, which the owner declined in favour of a clean room.
