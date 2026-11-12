# ADR-0005: Layered crates and their dependency rules

- Status: Accepted
- Date: 2026-11-11
- Phase: P01

## Context

- **The plan divides the backend into layers** ([§5.2](../implementation-plan.md#52-layers-and-crates)). The algorithms can then be tested without devices, the devices without a server, and the web framework appears in one crate only.
- **Cargo refuses only cycles.** Any other edge compiles. Without a check, a convenient dependency across layers creeps in: a device crate reaching into the API to emit an event, say. The layers then erode with no one deciding it.
- **Some crates in one layer build on each other.** The image algorithms run on the compute backends. Stitching and volumes use registration and focus measures from the image algorithms. The simulator implements the device traits.
- **`teta-wot` is pinned in one place** ([ADR-0004](0004-teta-wot-v0-2-0-extended-upstream-first.md)). If its types spread to lower crates, every upgrade touches them all.
- **Unsafe code is allowed in five crates only** ([plan §4.3](../implementation-plan.md#43-unsafe-code)). The workspace forbids it through its lints. But a crate that leaves out `[lints] workspace = true` loses that forbid without warning.

## Decision

**1. The crates and their layers.** The M3 crates are listed now, so their place is fixed before they exist.

| Layer | Crates |
|---|---|
| L0 Foundation | `oscope-core` |
| L1 Compute and algorithms | `oscope-compute`, `oscope-shm`, `oscope-imaging`, `oscope-stitch`, `oscope-volume` |
| L2 Devices and data | `oscope-hal`, `oscope-sim`, `oscope-grbl` (M3), `oscope-tucsen` (M3), `oscope-store` |
| L3 Engines | `oscope-jobs`, `oscope-pipeline`, `oscope-ext`, `oscope-ext-sdk` |
| L4 Services | `oscope-acquire`, `oscope-workflows` |
| L5 Interface | `oscope-api` |
| L6 Applications | `oscope-server` |

**2. The rules.** They hold for every kind of dependency: normal, dev and build.

- **A crate depends on crates in lower layers.** It may skip layers.
- **Within a layer, only these edges are allowed,** each one the plan implies:

  | From | To | Why |
  |---|---|---|
  | `oscope-imaging` | `oscope-compute` | Every algorithm has a CPU path and a CUDA path (§5.6) |
  | `oscope-stitch` | `oscope-compute`, `oscope-imaging` | Registration, on either backend |
  | `oscope-volume` | `oscope-compute`, `oscope-imaging` | Focus measures and FFTs, on either backend |
  | `oscope-sim`, `oscope-grbl`, `oscope-tucsen` | `oscope-hal` | They implement the device traits |
  | `oscope-pipeline` | `oscope-jobs` | Batch pipelines run as compute jobs (§5.7) |
  | `oscope-ext` | `oscope-jobs`, `oscope-pipeline`, `oscope-ext-sdk` | Extensions add nodes, run in workers, and are built against the SDK |
  | `oscope-workflows` | `oscope-acquire` | Autofocus and scans use the frame bus |

- **No layered crate depends on another workspace member,** such as an example or `xtask`. Examples and tools may depend on any crate.
- **Only `oscope-api` depends on `teta-wot`.** No crate depends on `axum` directly; `oscope-api` uses `teta-wot`'s re-export (ADR-0004).
- **Every crate takes the workspace's lints,** which forbid unsafe code. The exceptions are the crates of plan §4.3: `oscope-compute`, `oscope-shm`, `oscope-ext`, `oscope-ext-sdk` and `oscope-tucsen`.
  - They repeat the workspace's lints, with `unsafe_code` allowed.
  - They also turn on clippy's `undocumented_unsafe_blocks`, so an `unsafe` block without a `SAFETY:` comment fails CI.

**3. The check.**
- **The tables** are in `xtask/src/lib.rs`.
- **`xtask/tests/layering.rs`** runs `cargo metadata --no-deps` and checks every workspace member against them. It is part of `cargo test --workspace`, so CI runs it on every pull request.
- **Each broken rule** is reported in one sentence that names the edge.

**4. Changing the rules.**
- **A new crate, or a new edge within a layer,** changes the tables in the pull request that needs it, and its notes give the reason.
- **Moving a crate to another layer, or allowing an upward edge,** needs a new ADR.

## Consequences

- **Gains:**
  - The layers are checked on every pull request, not only in review.
  - A lower crate can't reach up for a quick fix; the code has to go up, or the interface down.
  - A crate can't lose the unsafe-code forbid by accident.
  - `teta-wot` upgrades stay inside `oscope-api`.
- **Costs:**
  - Every new crate edits the tables, and so does every edge within a layer.
  - Dev-dependencies are held to the same rules. A test that needs a helper from a higher layer belongs in that higher crate, or in an example.
- **To watch:** the edges within L1 and L3 are the plan's best reading today. P06 (compute), P21 (pipelines) and P24–P26 (extensions) may need to change them.

## Alternatives considered

- **Finer layers instead of edges within a layer:** L1a compute, L1b imaging, L1c stitching and volumes, and so on. That makes more layers than §5.2 has, and a crate would change layer whenever it needed a new edge.
- **A list of allowed dependencies for every crate:** explicit, but 17 lists to keep in step, and the idea of layers would be lost.
- **`cargo-deny`'s `bans`,** which can limit who may depend on a crate (`wrappers`). That covers the `teta-wot` rule, but not layers: each crate would need its own list of dependents.
- **Rules in documentation only:** they erode, as the context says.
- **`#![forbid(unsafe_code)]` in each crate's root:** just as easy to forget, and repeated in every crate. The workspace lint and a check of the manifests keep it in one place.
