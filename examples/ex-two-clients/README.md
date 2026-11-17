# ex-two-clients

P49's security ([ADR-0031](../../docs/adr/0031-authentication-and-the-control-lease.md)): two people share the simulated microscope, one at a time in control.

1. **Two accounts,** `alice` and `bob`, are made in the example's data root, their passwords hashed with argon2id.
2. **Everything under `/api/` needs a login** but logging in: `POST /api/v1/auth/login` gives a session's token, sent as `Authorization: Bearer …`. Pages and `/health` are open.
3. **The control lease:**
   - Anyone logged in can watch, and read everything.
   - Only the lease's holder can change anything: move the stage, start a job, write a property. Everyone else gets a 403.
4. **The script:**
   - Alice takes control, homes the stage and moves it.
   - Bob's move is refused. He asks for control, and Alice hands it over.
   - Now Bob can move the stage, and Alice can't.
5. **The page** logs in as either, and shows the stage and who has control. With it you can take, ask for, give up and hand over control, and move the stage.

## How to try it

```sh
cargo run -p ex-two-clients
```

It serves at http://127.0.0.1:8213/, and prints:

```text
ex-two-clients: alice took control (200), homed the stage and moved it (200); bob was refused (403)
  the requests ["bob"]; alice handed it over to "bob"; bob moved (200), alice was refused (403)
  open http://127.0.0.1:8213/ twice, and log in as alice in one and bob in the other
```

Open the page in two windows, and log in as `alice` (password `alice's password`) in one and `bob` (`bob's password`) in the other:
- Whoever doesn't have control sees "take control first" when they try to move the stage.
- "Ask for control" shows the request to the other window, and "Hand over" passes control to the one chosen.
- Control goes after 10 minutes without a change.

**With the server,** accounts are the command line's:

```sh
oscope-server account add alice     # asks for the password twice
oscope-server account list
oscope-server account passwd alice
oscope-server account remove alice
```

With an account, the server asks for logins:
- **Listening on the network** (`[server] address = "0.0.0.0"`) needs an account, and serves over TLS. Its certificate comes from `[security] certificate` and `key`; without them, a self-signed one is made once in `data_root/tls`.
- **The desktop app** proves itself with the local token (ADR-0015). It can always take control, with `take_control`.

## Its checks

`cargo run -p ex-two-clients -- --smoke` runs the script, then checks that:

- a request without a token gets a 401;
- Alice takes control and moves the stage, and Bob's move gets a 403;
- Bob's request shows in the lease, and the hand-over makes him its holder;
- Bob then moves the stage, and Alice gets a 403;
- the page and its script are served.
