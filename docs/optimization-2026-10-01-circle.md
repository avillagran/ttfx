# Circle-coordinate deduplication — 2026-10-01

`find_coords_on_circle` kept a hash set even when duplicates were allowed, and when uniqueness was required it checked membership and then inserted the same point. It now creates a set only when deduplication can be needed and uses `insert`'s result to decide whether to append a point. Coordinate arithmetic, iteration order, the public API and vector growth are unchanged.

The helper is shared by blackhole, rings and swarm in both engines, and by bubbles in the original engine. The gain is local to the helper: complete animations are unchanged within measurement noise.

## Helper

All 48 focused cases improved, by 1.148–5.017× throughput and 13.8–79.7% fewer user-space cycles. Allocation calls and requested bytes fell in 30 cases and were unchanged in the other 18.

| Radius / limit / unique | Time per call | User cycles | Allocation calls | Requested bytes | Throughput gain |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 / 1 / true | 82.27 → 37.73 ns | 228.76 → 99.46 | 2 → 1 | 148 → 64 | 2.180× |
| 1 / 2 / true | 180.49 → 156.17 ns | 475.97 → 407.61 | 2 → 2 | 148 → 148 | 1.156× |
| 10 / automatic / true | 9.69 → 7.99 µs | 25,579 → 21,025 | 11 → 11 | 6,364 → 6,364 | 1.214× |
| 100 / automatic / true | 84.08 → 66.30 µs | 231,483 → 185,980 | 18 → 18 | 67,596 → 67,596 | 1.268× |
| 100 / 1000 / false | 124.26 → 24.77 µs | 328,652 → 66,654 | 19 → 9 | 102,428 → 32,704 | 5.017× |

## Complete animations

None of the 37 effects shows a statistically detected change at 200×50, seed 1, virtual time, no pacing and output to `/dev/null`, and allocation counts and requested bytes are identical for every effect. An independent rerun on a Ryzen 9 9955HX measured blackhole, rings, swarm and bubbles at 0.98–1.02× in both build orders.

## Method

Baseline `921bd55`, Rust 1.98.1, Intel Xeon E5-2696 v4 under WSL2, both builds from the same workspace with locked dependencies, LTO and one codegen unit, pinned to one CPU. Each case ran two warmups and 17 alternating samples per build, with `perf stat` for cycles and CPU time and allocation tracing in separate runs. The full 48-case and 37-effect tables, the confidence-interval method and the benchmark harness are in this file as of commit `afcf3c7`.

Output is unchanged: 704 exact coordinate-vector comparisons against the previous implementation, including negative, zero and extreme `i64` arguments, and 2,664 complete CLI output comparisons across all effects, two seeds, six inputs, three colour modes and both rendering thread modes.
