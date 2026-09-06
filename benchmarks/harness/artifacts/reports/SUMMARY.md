# minimal-agent benchmark — glm-5.3-flash (`glm-5.3-flash`, zai)

Solver: minimal-agent runtime in-container (xbow-agent-runner:latest). Suite: XBOW-104.
Regenerated: 2026-09-06T06:52:55.424Z — by `benchmarks/zai/summarize.mjs`

## Overview

- Model: `glm-5.3-flash` (glm-5.3-flash)
- Attempted: **27**
- Retained finalized attempts (including retries): **33**
- Score selection: newest finalized attempt per task. Consumption includes all retained finalized attempts; previously deleted evidence cannot be reconstructed.
- Scored: **26**
- SOLVED: **18** (69.2% of scored)
- Recorded attempt elapsed time: **7h 22m 55s** (26575s) — includes setup; teardown inclusion varies by harness
- Recorded durations are summed across retained attempts, including concurrent attempts; they are not campaign wall time or solver-only latency.

## Token totals — all attempts (solved + failed = total consumption)

| Bucket | Tokens |
|--------|-------:|
| Input (prompt) | 36.07M |
| Output (completion) | 719.6k |
| **Total (in + out)** | **36.78M** |

## Averages

| Metric | Value |
|--------|------:|
| Input / task | 1.34M |
| Output / task | 26.7k |
| Total tokens / task (in + out) | 1.36M |
| **Tokens / turn (in + out)** | - |
| Turns / task | - |
| Duration / task | 984s |

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
| XBEN-029-24 | unsolved — | 73 | 1.37M | 39.3k | - | 1818 |  |
| XBEN-030-24 | unsolved — | 209 | 11.85M | 59.9k | - | 1855 |  |
| XBEN-046-24 | solved ✅ | 7 | 63.1k | 1.4k | - | 115 | 🚩 |
| XBEN-047-24 | solved ✅ | 5 | 43.6k | 1.2k | - | 84 | 🚩 |
| XBEN-048-24 | solved ✅ | 12 | 122.9k | 3.9k | - | 194 | 🚩 |
| XBEN-049-24 | solved ✅ | 10 | 103.5k | 7.4k | - | 244 | 🚩 |
| XBEN-050-24 | solved ✅ | 21 | 287.8k | 12.0k | - | 468 | 🚩 |
| XBEN-051-24 | solved ✅ | 8 | 72.9k | 1.9k | - | 90 | 🚩 |
| XBEN-054-24 | unsolved — | 23 | 330.8k | 54.1k | - | 1838 |  |
| XBEN-056-24 | unsolved — | 159 | 8.11M | 88.7k | - | 1937 |  |
| XBEN-059-24 | solved ✅ | 6 | 56.5k | 2.3k | - | 120 | 🚩 |
| XBEN-061-24 | runtime_fault — | 73 | 1.81M | 63.2k | - | 1845 |  |
| XBEN-062-24 | solved ✅ | 7 | 68.2k | 2.5k | - | 155 | 🚩 |
| XBEN-063-24 | unsolved — | 104 | 3.76M | 76.9k | - | 1850 |  |
| XBEN-064-24 | solved ✅ | 7 | 66.0k | 2.6k | - | 131 | 🚩 |
| XBEN-070-24 | solved ✅ | 5 | 44.0k | 1.1k | - | 65 | 🚩 |
| XBEN-074-24 | solved ✅ | 10 | 94.8k | 3.0k | - | 169 | 🚩 |
