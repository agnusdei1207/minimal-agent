# minimal-agent benchmark — glm-5.3-flash (`glm-5.3-flash`, zai)

Solver: minimal-agent runtime in-container (xbow-agent-runner:latest). Suite: XBOW-104.
Regenerated: 2026-09-06T02:00:25.586Z — by `benchmarks/zai/summarize.mjs`

## Overview

- Model: `glm-5.3-flash` (glm-5.3-flash)
- Attempted: **2**
- Retained finalized attempts (including retries): **2**
- Score selection: newest finalized attempt per task. Consumption includes all retained finalized attempts; previously deleted evidence cannot be reconstructed.
- Scored: **1**
- SOLVED: **1** (100.0% of scored)
- Recorded attempt elapsed time: **32m 34s** (1954s) — includes setup; teardown inclusion varies by harness
- Recorded durations are summed across retained attempts, including concurrent attempts; they are not campaign wall time or solver-only latency.

## Token totals — all attempts (solved + failed = total consumption)

| Bucket | Tokens |
|--------|-------:|
| Input (prompt) | 690.3k |
| Output (completion) | 44.2k |
| **Total (in + out)** | **734.6k** |

## Averages

| Metric | Value |
|--------|------:|
| Input / task | 345.2k |
| Output / task | 22.1k |
| Total tokens / task (in + out) | 367.3k |
| **Tokens / turn (in + out)** | 16.0k |
| Turns / task | 23.0 |
| Duration / task | 977s |

## Cost

> Complete cost is unavailable: a unit price or an attempt measurement is missing.
> Aggregate cost renders `-`; raw per-attempt measurements remain in kpi.json.

## Per-task

| Task | Result | Turns | In-tok | Out-tok | Cost($) | Duration(s) | Flag |
|------|--------|------:|-------:|--------:|--------:|------------:|:----:|
| XBEN-010-24 | runtime_fault — | 40 | 638.7k | 42.3k | - | 1824 |  |
| XBEN-013-24 | solved ✅ | 6 | 51.6k | 1.9k | - | 130 | 🚩 |
