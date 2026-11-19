# ADR-0044: Things in the app: live properties and events

- Status: Accepted
- Date: 2026-11-18
- Phase: W1.2

## Context

- **The app shows the instrument's state as it is,** and the instrument changes without the app: another client moves the stage, an action runs on.
- **The server describes each Thing** in its Thing Description, at `thing_descriptions/`: its properties, actions and events, and how each is reached.
- **Each Thing has a WebSocket,** `/api/v1/{thing}/ws` ([ADR-0004](0004-teta-wot-v0-2-0-extended-upstream-first.md)):
  - a client asks with `addPropertyObservation` and `addEventSubscription`;
  - the server tells `propertyStatus` and `event` messages;
  - a name that isn't there, or can't be observed, gets an error `response`;
  - under security, the socket's first message is `{"authorization": "Bearer …"}` ([ADR-0031](0031-authentication-and-the-control-lease.md)), since a browser can't give a WebSocket a header;
  - the server closes it when it shuts down, and drops it when it dies.
- **Few properties can be observed.** Only data properties are: the stage's speed, the preview's look, saved settings. Most of a device's state is a functional property, read from the hardware when asked: the stage's position and status, the camera's exposure and gain.
- **Events say when that state changes:** the stage's `moved` and `alarm`, the camera's `mode_changed`, the status's `changed`.

## Decision

**1. The descriptions are read once,** for each server, and again only after a failure. A property is observable if its description has an `observeproperty` form, and writable if it has a `writeproperty` form and isn't read-only.

**2. One WebSocket for each Thing in use,** shared by all that follow the Thing (`things/socket.ts`).
- **It opens** with its first listener, and closes after its last.
- **It asks for everything it was asked,** each name once, and again each time it opens.
- **Closed by the server,** it opens again after 250 ms, then twice as long each time, up to 10 s. Once it has stayed open for 5 s, the next wait is 250 ms again.
- **Under security,** its first message is the session's token.

**3. A Pinia store for each Thing** (`things/store.ts`) keeps its properties live:
- **an observable property** is read once, then kept by the socket;
- **any other** is read at intervals, once a second unless asked otherwise, with one reader at the quickest interval anyone asked for. It is read again whenever its Thing tells an event, since the event may be its change;
- **a write shows at once.** Taken, it is kept, and a property the socket doesn't keep is read again, for what the device made of it. Refused, the last value comes back, with the server's reason;
- **a value is stale** once the server hasn't confirmed it for 5 s. An open socket confirms what it keeps, since it would have told a change; a read confirms what it reads.

**4. Components use them through composables,** typed by their paths, as W1.1's calls are ([ADR-0043](0043-generated-api-types-and-the-typed-client.md)), and kept for as long as the component lasts (`things/use.ts`):
- **`useProperty("/api/v1/stage/position")`:** its value, whether it is stale, a write's refusal, and `write`, which takes only what the property's `PUT` does;
- **`useEvent("/api/v1/stage/moved")`:** each event as it comes, in order, the newest few kept;
- **`useAction("/api/v1/stage/home")`:** `run`, its invocation as it goes, then its output or its error, and `cancel`. Leaving the component stops following the action, and doesn't cancel it.

## Consequences

- **Gains:**
  - A page shows what the instrument does, whoever made it do it, without reloading.
  - A value the server can't confirm says so.
  - A refused write never leaves the page showing a value the instrument doesn't have.
- **Costs:**
  - A functional property is as fresh as its interval, between events: the stage's position is read four times a second while the page shows it.
  - Every followed property that isn't observed is read again at each of its Thing's events, even an event that didn't change it.
  - A socket per Thing in use, not one for the whole server.
- **To watch:**
  - **teta-wot telling functional properties' changes:** then the store would keep them by the socket too, and read less.
  - **A Thing with many properties followed at once:** one event then means as many reads.

## Alternatives considered

- **One socket for the whole server:** teta-wot v0.2.0 serves one a Thing.
- **Server-sent events,** `GET` with `Accept: text/event-stream`: one stream a property or event, where a socket carries a Thing's.
- **Reading only at intervals:** simple, but events would come late, and every value would cost a read whether or not it changed.
- **Taking a property's value from the event that announces it,** the position from `moved`: quicker, but it needs a map of which event carries which property, and a read is right whatever the event carries.
