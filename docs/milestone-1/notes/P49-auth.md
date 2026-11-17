# P49: Accounts, sessions, control lease, TLS

- Status: Done
- Branch / PR: `m1/p49-auth` / [#65](https://github.com/lucaswewa/OScope/pull/65)
- Size: L · estimated ~2,200 before it started (in `oscope-api`, accounts, sessions, the lease, the authenticator and authoriser, the `auth` Thing ~720; in `oscope-server`, the account commands, TLS and the network's rules ~280; tests ~610; `ex-two-clients` ~280; a tenth on top); one pull request, TLS within it, at the owner's standing choice · measured **1,768** hand-written lines added (code 941: `oscope-api` 730, `oscope-server` 211; tests 487; examples 327, `ex-common`'s `call_as` among them; glue 13), and 15 removed. **That is 868 over L's 900, and 20 % under the estimate:** teta-wot's hooks did more of the work than the estimate allowed

## Summary

Safe remote use: logins, one controller at a time, and encrypted links ([spec](../phases.md#p49); plan §5.18, D22; [ADR-0031](../../adr/0031-authentication-and-the-control-lease.md)).

- **Accounts:** argon2id hashes in `{data_root}/accounts.json`, readable only by the server's account, and `oscope-server account add|remove|passwd|list`.
- **Sessions:** bearer tokens from `POST /api/v1/auth/login`, ended by logging out or by going unused. A WebSocket sends its token in its first message: teta-wot's check, through OScope's authenticator.
- **The local token** of ADR-0015 is the desktop app's principal, `local`.
- **The control lease:**
  - Anyone logged in may read and watch; only its holder may change anything.
  - It is acquired, asked for, handed over and released. It goes when idle, and the desktop app can always take it.
- **A test made from the Thing Descriptions** checks every affordance that changes state, 74 of them, against a caller without the lease.
- **TLS:** on the network the server needs an account and serves HTTPS, with a certificate given or made once.
- **`ex-two-clients`:** two people share the microscope, one in control.

## What was built

- **`oscope-api::auth`:**
  - `Accounts`: open, add, passwd, remove, verify, names;
  - `Security`, with the sessions, the local token and the lease;
  - `Idle`, how long each may idle;
  - the authenticator `Guard`, and the authoriser `LeaseCheck`;
  - the `auth` Thing:
    - `POST login` and `POST logout`;
    - `me`, `lease` and `accounts`;
    - `acquire_control`, `request_control`, `release_control`, `hand_over_control` and `take_control`;
    - the event `lease_changed`.
- **`oscope-api`'s server:** `ApiBuilder::security`, and `Api::serve_tls` on teta-wot's `TlsListener`. The `tls` feature of teta-wot is on.
- **`oscope-server`:**
  - `[security]`: `certificate`, `key`, `session_idle_minutes` (480) and `lease_idle_minutes` (10);
  - the `account` commands, a password asked for twice and unseen on a terminal, or stdin's first line;
  - accounts turn security on;
  - an address that isn't loopback needs an account, and TLS: given, or `certificate()` makes it in `data_root/tls`.
- **`ex-common`:** `call_as`, a request with a session's token.
- **`ex-two-clients`** (port 8213, shared with `ex-logs`), its page and README.
- **Dependencies:**
  - `argon2` 0.6.0, `rcgen` 0.14.10 and `rpassword` 7.5.4;
  - teta-wot's `tls`, with rustls 0.23.46 and `ring` 0.17.14;
  - `tokio-rustls` 0.26.6, for the tests.

  `rustls-pki-types` and `rustls-webpki` are held at 1.15.1 and 0.103.16, as their newest are under two weeks old.

## How to try it

```sh
cargo run -p ex-two-clients
```

Then open http://127.0.0.1:8213/ in two windows. Log in as `alice` (`alice's password`) in one and `bob` (`bob's password`) in the other. Its README has the script and the server's commands.

```text
ex-two-clients: alice took control (200), homed the stage and moved it (200); bob was refused (403)
  the requests ["bob"]; alice handed it over to "bob"; bob moved (200), alice was refused (403)
  open http://127.0.0.1:8213/ twice, and log in as alice in one and bob in the other
```

## Performance

§5.20 has no budget for logins. argon2id's default parameters (19 MiB, 2 passes) take tens of milliseconds a login, in a blocking task. A request's check is a SHA-256 and a map lookup.

## Design notes and deviations from the plan

- **Security is on when there are accounts.** Without them, as in every example and test, nothing changes. A default account was not made, because a default password is worse than none on loopback.
- **Pages and `/health` are open:** a browser can't send a token for a page. Everything under `/api/` but logging in needs a token.
- **Login and logout are endpoints,** not actions, so each answers at once. Logging out as an action ended the session before its own result could be read.
- **Every action needs the lease,** queries among them, such as `simulation/in_focus`: teta-wot counts each action as a change. ADR-0031 notes it.
- **The browser's media** under security, images and `EventSource`, can't carry a bearer token: M2's web app fetches them, or M2 chooses cookies. ADR-0031 notes it.
- **Sessions are in memory:** a restart logs everyone out.
- **TLS on loopback** is used when a certificate is given, which is how the server's test tries it.
- **Not tried on the network here:** listening on 0.0.0.0 could ask the Windows firewall on the owner's desktop. The test checks the refusal before listening, and TLS on loopback.

### What the mutants' reading found

Twenty-six mutants, listed in advance from the code:

- **accounts:** names, short passwords, the decoy;
- **sessions:** expiry, use keeping one, logging out;
- **the authenticator:** the local token, logging in and pages left open;
- **the authoriser:** reading free, the `auth` Thing's own, the lease needed;
- **the lease:** idling, changes keeping it, taking it, release, requests, hand-overs by others and to strangers, the desktop app's override, a request cleared;
- **the server:** the network's account check, TLS with a certificate, the certificate kept, a password's line end.

Reading the tests against the list first found gaps, now covered:

- a capital inside a name;
- the decoy password tried on an unknown name;
- a session kept by use past its idle time;
- the lease kept by changes past its idle time;
- release and hand-over by someone who doesn't hold it, a hand-over to a stranger, and the request cleared after one;
- a password from stdin, logged in with.

The run then needed one fix in a test. With TLS turned off, the server answered plain HTTP and kept the connection open, so the test waited rather than failing. It now asks for the connection to close, and that mutant fails it at once.

So **25 of the 26 were run, and all 25 are caught.** One was not run: the server listening on the network without an account. It would listen on 0.0.0.0, and could ask the Windows firewall on the owner's desktop.

### Two cores

`scripts/two-core.sh`, five times each, side by side, on two CPUs each:

| Target | Passed | Time |
|---|---|---|
| `oscope-api:auth` | 5 of 5 | 20.3–20.4 s |
| `oscope-api:api` | 5 of 5 | 0.2–2.1 s |
| `oscope-server:cli` | 5 of 5 | 0.9–2.5 s |
| `oscope-server:sidecar` | 5 of 5 | 0.6–1.1 s |
| `oscope-server:config` | 5 of 5 | 0.2–1.0 s |
| `xtask:layering` | 5 of 5 | 0.3–0.5 s |
| `ex-two-clients --smoke` | 5 of 5 | 18.6–20.2 s |
| `ex-http-walkthrough --smoke` | 5 of 5 | 17.6–19.2 s |

The auth suite's time is mostly waiting out idle times on purpose, of 2 and 4 s.

## Tests

| Suite | Tests | What they show |
|---|---|---|
| `oscope-api` `auth` (new) | 5 | **Accounts:** argon2id hashes, never the passwords; 0600 on Linux; read again; passwords changed; names, short passwords, doubles and unknown accounts refused; the decoy lets no one in. **Sessions:** 401 without a token, with a challenge; a wrong password; `me`; logging out; a session kept by use past its idle time, and ended by idleness. **The lease:** reading free, changing refused; acquired; taken from no one; asked for; released and handed over only by its holder, to an account; the request cleared; the desktop app's override; kept by changes past its idle time, lost when idle. **Every state-changing affordance** in the Thing Descriptions, 74 of them, refuses a caller without the lease, and works with it. **TLS:** HTTPS to a trusted certificate, and no answer to plain HTTP |
| `oscope-server` `cli` (two new) | 10 | **The account commands**, from stdin, the password without its line end, and their refusals. **On the network** without an account, refused before listening. **A certificate** made once, its key 0600 on Linux, served over HTTPS |
| `ex-two-clients --smoke` | — | 401 without a token; the script's 200s and 403s; the request, the hand-over; the page |

**The local replays,** the merge gate (ADR-0038), each run alone on `a53b6f4`. The commit since adds only these lines and the measurements above.

- **Windows:** `scripts/ci-local.sh`, in 6 minutes: 66 of 66 steps, 604 tests in 227 suites, the Python tests real, and every example's `--smoke`. No CUDA path changed, so the CUDA tests didn't run.
  - Its first run failed one step under its load: `ex-live-recipe`'s smoke run, whose recipe took 204.2 ms against its budget of 200 ms. P49 changes nothing it runs.
  - Alone on two cores it passed 8 of 8, and the run again passed whole.
  - A budget that holds under load is a follow-up.
- **Linux:** `scripts/ci-clone.sh` on a fresh WSL clone, in 9 minutes: 66 of 66 steps, 604 tests in 227 suites, the accounts' file and the key at 0600; 845 files, none with a CR.

## Follow-ups and known gaps

- **Media and events in a browser under security:** M2's web app.
- **Actions that change nothing,** marked so in teta-wot, so a watcher may call them.
- **Roles,** if M3's lab needs them: the authoriser, from the principal's attributes.
- **Sessions kept across restarts,** if wanted.
- **`ex-live-recipe`'s smoke budget,** which a loaded replay overran once by 4 ms.

## Clean room

Nothing was copied from `microscope` or any other codebase. Passwords use the `argon2` crate's argon2id (RFC 9106), with its defaults, which OWASP's password storage advice asks for. TLS is rustls's, through teta-wot, and the certificates are rcgen's.
