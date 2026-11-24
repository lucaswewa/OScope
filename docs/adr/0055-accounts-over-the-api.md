# ADR-0055: Accounts over the API

- Status: Accepted
- Date: 2026-11-24
- Phase: W8.6

## Context

- **Accounts came in M1 from the command line alone** (P49, [ADR-0031](0031-authentication-and-the-control-lease.md)): `oscope-server account add`, `passwd` and `remove` change `{data}/accounts.json`, and the server reads that file at its start. With an account it has security, with none it hasn't, and on the network it refuses to start without one.
- **Milestone 2 keeps them from the app** (D22, W8.6): listed, added, removed, and a password changed.
- **The `auth` Thing** logs in and out and keeps the lease. It is served only with security, so a server without accounts has nowhere to add its first.
- **Security can't be turned on while the server runs.** Every request is checked from the start or none is, and turning it on mid-run would lock every open page out at once.

## Decision

- **An `accounts` Thing, served by every server,** with security or without, keeps the accounts over the API:
  - **its accounts** are the file the server reads at its start, the same accounts security checks logins against when it is on, so an account added logs in at once;
  - **`names`, `secure` and `restart_needed`:** the accounts, whether this start has security, and whether the next start has it otherwise;
  - **`sessions` and `idle`:** the sessions open, each its account and how long it has been idle, without its token; and how long sessions and the lease may be idle, and the lease's holder has been. None without security.
- **Adding and removing need the lease,** as every change of the instrument's state does (ADR-0031). On a server without security, anyone may, as with everything else there.
  - **The first account,** added on a server without security, **turns it on at the next start:** `restart_needed` says so, and the app offers the restart (W4.3).
  - **The last account can't be removed while the next start listens on the network,** as the configuration file says (P52), which needs accounts. Removed otherwise, it turns security off at the next start.
  - **An account removed ends its sessions,** the caller's own among them.
- **One's own password is changed with the old one,** and without the lease: it isn't the instrument's. It needs a login as that account; an account's password is changed without the old one only from the command line.
- **Rules stay the store's:** a name of 1 to 64 of `a`–`z`, `0`–`9`, `.`, `_` and `-`, starting with a letter, not `local`; a password of 8 characters at least; hashing with argon2id, off the async threads.
- **The command line stays,** for a server that isn't running, and for a forgotten password.

## Consequences

- **Easier:** a first account, and later ones, from the app; the sessions open and the idle times seen.
- **Harder:** security still waits for a restart, which the app says and offers. An account removed takes its sessions with it at once, its holder's page logged out.
- **All accounts have equal rights,** as in M1: anyone with the lease may add or remove any account, their own among them. Roles would be a new decision.

## Alternatives considered

- **The `auth` Thing, served always:** its logins and lease mean nothing without security, and its description would describe them on every server.
- **The `settings` Thing:** accounts aren't settings of the next start alone. An account added logs in at once on a secured server.
- **Turning security on at once:** every open page would be locked out mid-run, and the local token, which the server writes at its start, would be missing.
