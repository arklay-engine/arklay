# Agent guide

Instructions for coding agents (and humans) working in this repository. The
rest of the documentation is in `README.md` and `docs/`.

## Building and testing: keep the compiler cheap

The `arklay` crate is large (about 250k lines), so every cargo command that
recompiles it costs tens of seconds. Most of a session's wall-clock time used
to go to the compiler (#266). Follow these rules:

- **Use one profile for everything: `--profile fast-release`.** Tests, builds
  and runs all use it: `cargo test --profile fast-release ...`,
  `cargo build --profile fast-release`,
  `cargo run --profile fast-release -- <args>`. The binary is
  `target/fast-release/arklay`. Each other profile (plain `cargo test`,
  `cargo build`, `--release`) compiles the whole crate again from scratch in
  its own directory. Do not use `--release` unless the task is about the
  shipped binary; it adds a slow LTO link.
- **Build only the target you need.** Most tests are unit tests in the
  library, so iterate with
  `cargo test --profile fast-release --lib <filter>`. A bare
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
  In other shells, on Linux, run
  `export RUSTC_WRAPPER="$PWD/scripts/rustc-wrapper.sh"` from the repository
  root.
- **Claude Code sessions do all of this automatically.** In a worktree,
  `scripts/session-start.sh` (hooks in `.claude/settings.json`) sets the
  wrapper, seeds `target/`, and builds the tests and clippy in the
  background: at session start, and again after each tool call that changes
  a source file (log: `target/prebuild.log`). So by the time you run cargo,
  the build is usually done. Do not run the seed script again. If a cargo
  command prints "Blocking waiting for file lock", it is waiting for that
  background build: let it finish. Do not kill it, and do not start other
  builds to work around it.
- **Edit and build in separate tool calls.** Make edits with the Edit tool
  or a command that only edits, and run cargo in a later call. The
  background build starts as soon as an edit lands, so it compiles while you
  write the next command. An edit and `cargo test` in one command
  (`python3 - <<EOF ... EOF; cargo test ...`) gives it no head start.
- **Do not run `cargo clean`** or delete `target/` unless a build is
  genuinely broken. A cold build takes minutes.
- Inserting or removing lines near the top of a big file (`src/engine.rs`,
  `src/game.rs`) shifts every span below it and costs a bigger rebuild than an
  edit that keeps the line count. That is no reason to avoid the edit, but
  batch related edits before rebuilding instead of rebuilding after each one.

## Engine rules

- **Match the original.** The aim is to play exactly as the original games
  do. When the port knowingly differs, file a GitHub issue and name it in a
  comment at the site (`(#123)`); do not write deviation pages. RE 2's
  differences still being ported are listed in `oracle/re2-deviations.md`,
  which the regression runner's notes cite: keep it current when an RE 2
  change closes or adds one.
- **Ticks are deterministic.** Gameplay runs at a fixed 30 Hz. Nothing inside
  a tick may read the wall clock or other outside state: a pack, room, input
  and tick count give byte-identical frames, states and saves on every run
  and platform.
- **Pack and mod data is untrusted.** Every parser returns `Err` on arbitrary
  bytes: never a panic, abort, unbounded allocation or unbounded loop. New
  formats get a torture-matrix row (`tests/torture.rs`) and a fuzz target
  (`fuzz/`). Allocation caps live in `src/budget.rs`; when a legitimate
  input trips one, raise that format's cap to at least twice the observed
  value, never a global cap.
- **Lua is sandboxed and enemy scripts are stateless.** No `os`, `io`,
  `package` or `debug`, and a fixed memory ceiling. Enemy scripts
  (`enemy/{id}/enemy.lua`, `lib/**.lua`) keep all mutable state on the Rust
  entity, so resetting their VM between two updates changes nothing.
- **Fixed layouts stay fixed.** The save block is the original's 0x800 bytes,
  field for field. Entity slots are never compacted: slot 0 is the player,
  and every id the original dispatches allocates (a missing script or model
  parks inert).
- **Real-asset tests** are ignored tests in one binary, `tests/real/main.rs`:
  `ARKLAY_RE1_ROOT=/path/to/re1 ARKLAY_RE1_PACK=/path/to/re1.akpak cargo test
  --profile fast-release --test real <module>:: -- --ignored`. The RE 2
  tests (`re2_real`) read `ARKLAY_RE2_ROOT` instead. Run the module your
  change touches, not the whole suite.

## Automating arklay: driver scripts

To run the engine for a task (check a room, reproduce a bug, measure
something, capture a frame), write a Lua driver script and run it with
`arklay script --headless FILE` (or `-` to read it from stdin). The script
opens a pack, boots a session, holds buttons, ticks and reads or writes the
game state; the API is `lua-types/script.lua`
(`docs/scripting/lua/script.md`).

```sh
target/fast-release/arklay script --headless - <<'LUA'
load_game_pack("re1.akpak")
load_room("10A")
hold("forward")
tick(30)
print(json({ room = game:room(), pos = { game:player_pos() } }))
LUA
```

- **Always pass `--headless`.** Without it the run opens a window and fails
  where none can open.
- **Do not add command-line flags for automation.** When a script needs
  something the API lacks, add it to the script API (`src/engine/script.rs`)
  or, for game state, to the hook API (`impl UserData for GameState` in
  `src/lua.rs`, which `game` forwards to), and document it in the matching
  `lua-types/*.lua`.
- Only `print` writes to stdout; parse `print(json(...))`. A non-zero exit
  status is a real failure: a Lua error, a failed engine call, or an engine
  warning (`strict(false)` lets warnings through).

## The check tiers

`scripts/check.sh` has tiers. Run the session tier once, before pushing. Do
not run it after every edit.

| Tier | Command | When | Budget |
|---|---|---|---|
| Session | `./scripts/check.sh` | once per task, before pushing | 5 min cold, much less warm |
| CI | `./scripts/check.sh --ci` | runs on every PR in GitHub Actions; run locally only when debugging a CI-only failure | |
| Release | `./scripts/check.sh --full` / `--soak`, `--test perf_real` | release cycle only, or when the task explicitly asks | 30+ min |

- The session tier is fmt, clippy, the unit and fast integration tests, the
  tests of the library packages (`crates/arklay-routing`,
  `crates/arklay-biorand`), and the docs build (`cargo doc`, whose lints CI
  enforces), all in `fast-release`. It prints each step's time and what it
  skipped.
- **Run it unfiltered:** `./scripts/check.sh`, with no `grep` or `tail`. It
  prints one line per step and the end of a failing step's log. Its last line
  is the verdict (`all green` or `FAILED at: ...`), and its exit status is
  non-zero on failure. The full logs are in `target/check-logs/`. Do not run
  it a second time to see what the first run printed.
- **For RE 2 changes, also run the regression runner** (once, before pushing, after the session tier):
  `scripts/re2-regress.py --out DIR` (about 6 minutes; `DIR` is not under `/tmp`),
  which checks arklay against the stored recordings of the original. See
  "The regression runner" in `oracle/re2.md`.
- Leave the debug test build, `--no-default-features`, the fuzz compile check,
  Windows, MSRV and the artifact smoke to CI.
- **Never run `--full`, `--soak` or the perf tests unless the task asks for
  them.** They need the real game assets and take half an hour or more.
