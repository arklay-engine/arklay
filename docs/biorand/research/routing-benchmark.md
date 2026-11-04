# Routing benchmark: C# and Rust

Measured 2026-11-03 on an AMD Ryzen 9 7950X VM (8 vCPUs), pinned to one CPU.
The C# build is BioRand.Common 1.6.1, built in Release with .NET 10.0.403.
The Rust build is `arklay-biorand` (`--release`, thin LTO) with rustc 1.99.1.

Both sides generate routes for common's RE2 Leon A example graph (330 nodes,
333 edges, 47 keys) over 1000 seeds, after 100 warm-up seeds. Graph loading
is not counted. Each figure is the median across runs, with the range in
parentheses.

| Implementation | Runs | Mean per route | Median per route | p99 | 1000 seeds |
|---|---|---|---|---|---|
| C# 1.6.1, as shipped | 5 | 23.48 ms (23.10–24.34) | 22.67 ms | 29.05 ms | 23.5 s |
| C# 1.6.1, `ValidateState` removed | 3 | 6.62 ms (6.58–6.91) | 5.26 ms | 13.04 ms | 6.6 s |
| Rust, RE2 | 5 | 1.00 ms (0.98–1.01) | 0.965 ms | 1.23 ms | 1.0 s |
| Rust, RE1 Chris keys (graph rebuilt each time) | 3 | 0.800 ms | 0.796 ms | 0.890 ms | 0.8 s |
| Rust, RE1 Jill keys (graph rebuilt each time) | 3 | 0.811 ms | 0.808 ms | 0.944 ms | 0.8 s |

The Rust port is about 23 times faster than shipped C#, and 5–7 times faster
on the algorithm alone. Building the graph takes 0.36 ms warm in Rust and
5.6 ms in C#.

- **C#'s `ValidateState`** runs the solver on every candidate, but it can never
  reject one, because 1.6.1 never reports a potential softlock (see "Known
  issues" in `plan.md`). That check is about 72% of C#'s time. The port leaves
  it out.
- **The figures are aggregates.** The two random streams differ, so each seed
  gives a different route on each side. They agree on average: about 33.8 keys
  placed per route, every node reached, and no dead ends.
- **Different data structures:** C# uses immutable sets, the port uses bitsets.
  Both sides build a debug log as they go.

The benchmark harness is not kept in the repository.
