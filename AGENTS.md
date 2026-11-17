# Building

Use the `fast-release` cargo profile for non-distributed builds.

# Command line

No new CLI commands or options are to be added without the user's express permission.
Piping a Lua script into `arklay script --headless -` is the recommended way for an agent to control the game (see `lua-types/script.lua`).

# Testing

Testing, particularly for regressions, should be fully covered by unit tests
All unit tests can be run, but integration tests must be very selective based on the code touched
Very long test workloads should not be run for day to day work
Lua can be piped into Arklay as a means of ad hoc control

## Parity with OG

Arklay will produce a game pack for RE 1, 2, and 3. Each one should be as close as possible both visually and in behaviour (frame perfect) as the original PC ports of the games. This excludes any QoL alterations that Arklay provides, most of which can be disabled for the purpose of frame testing.

- RE 1 has a C++ reference codebase. This can be used for parity testing. This fork should be kept up to date with the upstream. But it's purpose is to store the parity testing code on top of [RE 1 decomp](https://github.com/IntelOrca/resident-evil-pc-decomp).
- RE 2 can only be checked using the sourcenext build natively on Windows, or wine on Linux with the classic rebirth patch on top.

Software renderering, 30 fps, fixed clock should be used for this.

# Documentation

`/docs` is strictly for the public engine documentation book only. It's purpose is to provide documentation for creating game packs, and mods for Arklay. It is NOT a dumping ground for documenting the internals of the engine.

No other docs related .md files should be added to the repo for any other reason. The code is the truth. No plans, no milestones, no results. The code should be self documenting.

# Review

Once code is proven to work:
- Run fmt and clippy
- Review changes with a sub agent, ideally from a different model family. E.g. if Opus 5.5 coded, 6.1 Sol (high) reviews it
- Resolve all issues reported
- Consider a second review iteration if many changes were applied after the first
