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
- **Seed a new worktree before its first build:** run
  `./scripts/seed-target.sh`. It reflink-copies the build output of the most
  recently built other worktree. That is instant on btrfs and XFS, and the
  script does nothing on other filesystems. The first build then compiles
  only this crate rather than every dependency. With
  `scripts/rustc-wrapper.sh` as `RUSTC_WRAPPER`, even this crate's build is
  incremental, so a new worktree's first session check takes about a minute.
  Claude Code sessions get the wrapper automatically through
  `.claude/settings.json`. In other shells, on Linux, run
  `export RUSTC_WRAPPER="$PWD/scripts/rustc-wrapper.sh"` from the repository
  root.
- **Do not run `cargo clean`** or delete `target/` unless a build is
  genuinely broken. A cold build takes minutes.
- The wrapper also passes rustc `-Zincremental-ignore-spans`, so inserting
  lines no longer recompiles all the unchanged code below the edit. The cost:
  a **panic's line number can be stale** by the number of lines inserted
  above it since that function was last compiled. Use the test name and the
  assertion message to find it. Compile errors and clippy's output are exact.
  `ARKLAY_EXACT_SPANS=1` turns this off (one full rebuild of the crate).
- Batch related edits before rebuilding instead of rebuilding after each
  one.

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
- **Run it unfiltered:** `./scripts/check.sh`, with no `grep` or `tail`. It
  prints one line per step and the end of a failing step's log. Its last line
  is the verdict (`all green` or `FAILED at: ...`), and its exit status is
  non-zero on failure. The full logs are in `target/check-logs/`. Do not run
  it a second time to see what the first run printed.
- Leave the debug test build, `--no-default-features`, the fuzz compile check,
  docs, Windows, MSRV and the artifact smoke to CI.
- **Never run `--full`, `--soak` or the perf tests unless the task asks for
  them.** They need the real game assets and take half an hour or more.
