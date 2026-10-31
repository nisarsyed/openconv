# Vendored: openmls_sqlite_storage 0.3.0

A copy of [`openmls_sqlite_storage`](https://crates.io/crates/openmls_sqlite_storage)
0.3.0 (openmls commit `3a3e35de`, MIT, see `LICENSE`), with one change:
`Cargo.toml` asks for `rusqlite` 0.40 and `refinery` 0.10 instead of 0.37 and
0.9.

That change is upstream already, in openmls#2256 (merged 2026-10-05), which
needed no code changes either. It has not been released. Until it is, this
copy lets the client core and the relay share one `rusqlite`:
`libsqlite3-sys` declares `links = "sqlite3"`, so a build graph can hold only
one version of it. See #107.

It is wired in through `[patch.crates-io]` in the workspace `Cargo.toml`, and
excluded from the workspace, so our lints and formatting do not apply to it.

**Delete this directory and the patch entry** once openmls publishes a
release of the storage crate on `rusqlite` 0.40, and depend on that instead.
Do not edit the source here: anything that is not upstream's would make
deleting it harder.
