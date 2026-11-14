# tests/ — fixtures, smoke and manual-verification data

This directory holds **shell/fixture data**, not a Cargo crate. The automated
cross-crate tests live in `crates/glaze-integration-tests/tests/` and run as part
of `cargo test --workspace`.

## Contents

| Path | Purpose |
|---|---|
| `fixtures/xdg-data/applications/` | A ready-made XDG data directory with real `.desktop` entries (visible, `NoDisplay`, `Hidden`, broken) for manual launcher checks. |
| `smoke.sh` | A quick end-to-end smoke script: portable `cargo test`, script syntax checks, and (on Linux) `scripts/check.sh`. |

## Manual launcher check with the fixture data

The fixture directory is an XDG data dir. Point the service/launcher at it and
confirm that only the visible entries appear:

```bash
export XDG_DATA_DIRS="$PWD/tests/fixtures/xdg-data:/usr/local/share:/usr/share"
# On Fedora, with the shell built:
scripts/run-nested.sh
```

Expected: `Firefox` and `Text Editor` are listed; `Nodisplay Demo`, `Hidden
One` and `Broken` are **not**. This mirrors the assertions in
`crates/glaze-integration-tests/tests/registry_e2e.rs`.

## Smoke test

```bash
tests/smoke.sh              # portable checks everywhere bash exists
tests/smoke.sh --with-check # additionally run scripts/check.sh on Linux
```

The smoke script never installs anything and never touches the session.
