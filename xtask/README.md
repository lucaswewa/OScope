# Workspace architecture checks

Run `cargo xtask check-layering` inside the development container, or run
`bash scripts/dev.sh backend` from the Linux checkout for the complete R02 gate.

The checker preserves the source layer table, same-layer exceptions, framework
restriction and unsafe-code boundaries. It reads all dependency kinds from Cargo
metadata and checks every currently ported workspace member. Future crate names in
the table define their intended layer; they are not placeholder workspace members.

The tests include deliberately invalid graphs. An upward dependency, an unlisted
same-layer edge, a framework dependency outside the API, a missing layer entry,
or omitted workspace lints must be rejected.

Legacy performance/soak commands depend on implementation that has not been ported
and are deferred to their relevant phase. See [the migration plan](../docs/migration-plan.md).
