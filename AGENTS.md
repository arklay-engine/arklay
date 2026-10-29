# Agent guide

Instructions for coding agents (and humans) working in this repository. The
rest of the documentation is in `README.md` and `docs/`.

## Building and testing: keep the compiler cheap

The `arklay` crate is large (about 250k lines), so every cargo command that
recompiles it costs tens of seconds. Most of a session's wall-clock time used
to go to the compiler (#266). Follow these rules:

- **Use one profile for everything: `--profile test-release`.** Tests, builds
  and runs all use it: `cargo test --profile test-release ...`,
  `cargo build --profile test-release`,
  `cargo run --profile test-release -- <args>`. The binary is
  `target/test-release/arklay`. Each other profile (plain `cargo test`,
  `cargo build`, `--release`) compiles the whole crate again from scratch in
  its own directory. Do not use `--release` unless the task is about the
  shipped binary; it adds a slow LTO link.
- **Build only the target you need.** Most tests are unit tests in the
  library, so iterate with
  `cargo test --profile test-release --lib <filter>`. A bare
  `cargo test <filter>` also builds and links the binary and every
  integration test binary. For integration tests, name the binary:
  `--test cli`, `--test real`, and so on.
- **Do not chain different builds** such as
  `cargo clippy ...; cargo test ...; cargo build --release; cargo run` in one
  command. Each step uses different settings and recompiles the crate. Run
  clippy once, at the end, through the check script.
- **Do not hide cargo's timing.** If you filter output, keep the
  `Finished ... in Xs` and `test result:` lines (for example
  `2>&1 | grep -E "^(error|warning)|Finished|test result|panicked"`), so the
  compile cost stays visible.
- **Do not run `cargo clean`** or delete `target/` unless a build is
  genuinely broken. A cold build takes minutes.
- Inserting or removing lines near the top of a big file (`src/engine.rs`,
  `src/game.rs`) shifts every span below it and costs a bigger rebuild than an
  edit that keeps the line count. That is no reason to avoid the edit, but
  batch related edits before rebuilding instead of rebuilding after each one.

## The check tiers

`scripts/check.sh` has tiers. Run the session tier once, before pushing. Do
not run it after every edit.

| Tier | Command | When | Budget |
|---|---|---|---|
| Session | `./scripts/check.sh` | once per task, before pushing | 5 min cold, much less warm |
| CI | `./scripts/check.sh --ci` | runs on every PR in GitHub Actions; run locally only when debugging a CI-only failure | |
| Release | `./scripts/check.sh --full` / `--soak`, `--test perf_real` | release cycle only, or when the task explicitly asks | 30+ min |

- The session tier is fmt, clippy, and the unit and fast integration tests in
  `test-release`. It prints each step's time and what it skipped.
- Leave the debug test build, `--no-default-features`, the fuzz compile check,
  docs, Windows, MSRV and the artifact smoke to CI.
- **Never run `--full`, `--soak` or the perf tests unless the task asks for
  them.** They need the real game assets and take half an hour or more.
