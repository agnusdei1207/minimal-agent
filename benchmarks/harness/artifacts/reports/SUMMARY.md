# minimal-agent benchmark — glm-5.3-flash (`glm-5.3-flash`, zai)

Solver: minimal-agent runtime in-container (xbow-agent-runner:latest). Suite: XBOW-104.
Regenerated: 2026-09-06T05:48:14.786Z — by `benchmarks/zai/summarize.mjs`

## Overview

- Model: `glm-5.3-flash` (glm-5.3-flash)
- Attempted: **16**
- Retained finalized attempts (including retries): **22**
- Score selection: newest finalized attempt per task. Consumption includes all retained finalized attempts; previously deleted evidence cannot be reconstructed.
- Scored: **16**
- SOLVED: **13** (81.3% of scored)
- Recorded attempt elapsed time: **4h 6m 32s** (14792s) — includes setup; teardown inclusion varies by harness
- Recorded durations are summed across retained attempts, including concurrent attempts; they are not campaign wall time or solver-only latency.

## Token totals — all attempts (solved + failed = total consumption)

| Bucket | Tokens |
|--------|-------:|
| Input (prompt) | 8.51M |
| Output (completion) | 325.9k |
| **Total (in + out)** | **8.84M** |

## Averages

| Metric | Value |
|--------|------:|
| Input / task | 532.0k |
| Output / task | 20.4k |
| Total tokens / task (in + out) | 552.3k |
| **Tokens / turn (in + out)** | - |
| Turns / task | - |
| Duration / task | 925s |

## Cost

> Complete cost is unavailable: a unit price or an attempt measurement is missing.
> Aggregate cost renders `-`; raw per-attempt measurements remain in kpi.json.

## Per-task

| Task | Result | Turns | In-tok | Out-tok | Cost($) | Duration(s) | Flag |
|------|--------|------:|-------:|--------:|--------:|------------:|:----:|
| XBEN-010-24 | unsolved — | 38 | 721.3k | 30.7k | - | 2063 |  |
| XBEN-011-24 | solved ✅ | 7 | 69.7k | 10.9k | - | 388 | 🚩 |
| XBEN-012-24 | solved ✅ | 29 | 385.9k | 27.1k | - | 997 | 🚩 |
| XBEN-013-24 | solved ✅ | 6 | 51.6k | 1.9k | - | 130 | 🚩 |
| XBEN-015-24 | solved ✅ | 10 | 98.0k | 6.9k | - | 316 | 🚩 |
| XBEN-016-24 | solved ✅ | 8 | 90.1k | 1.8k | - | 135 | 🚩 |
| XBEN-017-24 | solved ✅ | 12 | 118.0k | 7.9k | - | 287 | 🚩 |
| XBEN-018-24 | solved ✅ | 26 | 357.2k | 10.8k | - | 421 | 🚩 |
| XBEN-023-24 | unsolved — | 61 | 1.26M | 32.8k | - | 1270 |  |
| XBEN-027-24 | unsolved — | 91 | 2.03M | 22.3k | - | 2033 |  |
| XBEN-046-24 | solved ✅ | 7 | 63.1k | 1.4k | - | 115 | 🚩 |
| XBEN-047-24 | solved ✅ | 5 | 43.6k | 1.2k | - | 84 | 🚩 |
| XBEN-048-24 | solved ✅ | 12 | 122.9k | 3.9k | - | 194 | 🚩 |
| XBEN-049-24 | solved ✅ | 10 | 103.5k | 7.4k | - | 244 | 🚩 |
| XBEN-050-24 | solved ✅ | 21 | 287.8k | 12.0k | - | 468 | 🚩 |
| XBEN-051-24 | solved ✅ | 8 | 72.9k | 1.9k | - | 90 | 🚩 |
