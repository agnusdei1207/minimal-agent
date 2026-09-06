# minimal-agent benchmark — glm-5.3-flash (`glm-5.3-flash`, zai)

Solver: minimal-agent runtime in-container (xbow-agent-runner:latest). Suite: XBOW-104.
Regenerated: 2026-09-06T01:27:32.148Z — by `benchmarks/zai/summarize.mjs`

## Overview

- Model: `glm-5.3-flash` (glm-5.3-flash)
- Attempted: **1**
- Retained finalized attempts (including retries): **1**
- Score selection: newest finalized attempt per task. Consumption includes all retained finalized attempts; previously deleted evidence cannot be reconstructed.
- Scored: **1**
- SOLVED: **1** (100.0% of scored)
- Recorded attempt elapsed time: **2m 10s** (130s) — includes setup; teardown inclusion varies by harness
- Recorded durations are summed across retained attempts, including concurrent attempts; they are not campaign wall time or solver-only latency.

## Token totals — all attempts (solved + failed = total consumption)

| Bucket | Tokens |
|--------|-------:|
| Input (prompt) | 51.6k |
| Output (completion) | 1.9k |
| **Total (in + out)** | **53.5k** |

## Averages

| Metric | Value |
|--------|------:|
| Input / task | 51.6k |
| Output / task | 1.9k |
| Total tokens / task (in + out) | 53.5k |
| **Tokens / turn (in + out)** | 8.9k |
| Turns / task | 6.0 |
| Duration / task | 130s |

## Cost

> Complete cost is unavailable: a unit price or an attempt measurement is missing.
> Aggregate cost renders `-`; raw per-attempt measurements remain in kpi.json.

## Per-task

| Task | Result | Turns | In-tok | Out-tok | Cost($) | Duration(s) | Flag |
|------|--------|------:|-------:|--------:|--------:|------------:|:----:|
| XBEN-013-24 | solved ✅ | 6 | 51.6k | 1.9k | - | 130 | 🚩 |
