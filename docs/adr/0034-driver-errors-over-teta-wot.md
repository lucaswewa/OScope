# ADR-0034: Driver errors' HTTP statuses, and other names, over teta-wot

- Status: Accepted
- Date: 2026-11-12
- Phase: P16

## Context

- **ADR-0012 gives each driver error an HTTP status:**
  - a request that can't be met (`Unsupported`, `OutOfRange`, `Unknown`) is 400;
  - a device that can't do it now (`Busy`, `Stopped`, `Alarm`) is 409;
  - a device that isn't there (`Disconnected`) is 503;
  - a timeout is 504, and any other device failure (`Device`) is 502.

  P16 serves the devices as `teta-wot` Things.
- **`teta-wot` v0.2.0 answers every failed property with 500.** A getter's or setter's error is `PropertyError::Failed`, whose problem details always carry 500. Nothing lets a Thing choose another status.
- **An action's request is answered when it starts:** `201`, with its invocation. A failure ends the invocation as `error`, with a message, long after the request was answered.
- **`teta-wot`'s `Thing` trait has a `stop` method,** the Thing's own stopping. An action's name is its method's, so a Thing can't have an action called `stop`, and the stage's was to be called that (plan §5.4).
- **The owner's standing decision for P12–P23:** a change `teta-wot` would need is worked around in OScope, recorded in an ADR, and no upstream pull request is opened.

## Decision

**1. Property errors carry their status, and a layer answers with it.**
- A driver error in a property's getter or setter becomes a `PropertyError` whose message starts with a mark: `[device 409 busy] ` and the error.
- `oscope-api` adds a layer around every route, `devices::statuses`. When an answer is a 500, it reads the problem details, up to 64 KiB. If their `detail` has the mark, it answers instead with:
  - the mark's status;
  - the kind as the problem's `title`;
  - the error's own message as its `detail`, without the mark.
- Every other answer passes through unchanged.

**2. An action's error says its kind and status.** It ends the invocation as `error`, with `ActionError::handled("busy (409): …")`. A client reads them from the invocation's error. The request itself was answered `201`, as `teta-wot`'s actions always are.

**3. The stage's `stop` action is called `halt`.** The plan's `stop` would collide with every Thing's own `stop`. The camera's software trigger is `trigger_exposure`, beside its `trigger` property, for the same reason: one name per affordance.

**4. Upstream, later:** `teta-wot` could let a property error carry its status, and an affordance have a name of its own. Then the layer and the mark go, and `halt` can become `stop`. That is a follow-up, not part of M1.

## Consequences

- **Gains:**
  - Clients see ADR-0012's statuses on property writes and reads, with problem details whose `title` names the kind. P16's tests check each status.
  - The workaround is in one place: the mark, the layer and the two helpers, `property_error` and `action_error`.
- **Costs:**
  - Every 500 answer is read once more by the layer. They are rare, and capped at 64 KiB.
  - The mark is a convention between two pieces of OScope's code. A property message that started with `[device ` by chance would be rewritten. Only `property_error` writes it.
  - Actions don't answer with a status. A client that wants one waits for the invocation and reads its error.
  - The stage's stop is `halt`, unlike the plan's wording.
- **To watch:**
  - A `teta-wot` release that changes its problem details' shape, or adds statuses to property errors, makes this layer redundant or wrong. Its tests would show it.

## Alternatives considered

- **Answering 500, as `teta-wot` does:** simple, but clients couldn't tell a refused value from a broken device, as ADR-0012 wants them to.
- **Custom endpoints (`#[endpoint]`) for every write:** they can answer any status, but the properties would vanish from the Thing Descriptions, the OpenAPI document and observation.
- **Mapping refused values to 422 through `PropertyError::Invalid`:** it covers only one row of ADR-0012's table, and misnames a device's refusal as a validation error.
- **An upstream change first:** the owner chose to work around `teta-wot` during P12–P23.
