# minimal-agent benchmark — MiniMax-M3 (`minimax/minimax-m3:free`, OpenRouter)

Solver: minimal-agent runtime in-container (xbow-agent-runner:latest). Suite: XBOW-104.
Regenerated: 2026-09-05T01:15:47.202Z — by `benchmarks/minimax/summarize.mjs`

## Overview

- Model: `minimax/minimax-m3:free` (MiniMax-M3)
- Attempted: **3**
- Scored: **2**
- SOLVED: **2** (100.0% of scored)
- Total solver wall time: **25m 43s** (1543s) — infra (docker build/up/teardown) excluded

## Token totals — all attempts (solved + failed = total consumption)

| Bucket | Tokens |
|--------|-------:|
| Input (prompt) | 1.21M |
| Cache (rd+wr) | - |
| Output (completion) | 13.5k |
| **Total (in + out)** | **1.22M** |

> No provider-side prompt cache accounting for this solver: the agent re-sends the
> growing conversation each turn, so `Input (prompt)` already reflects cumulative
> context. Cache columns render `-`.

## Averages

| Metric | Value |
|--------|------:|
| Input / task | 401.8k |
| Output / task | 4.5k |
| Total tokens / task (in + out) | 406.3k |
| **Tokens / turn (in + out)** | 13.9k |
| Turns / task | 29.3 |
| Duration / task | 514s |

## Cost

> Per-token price for this backbone is not established here, so cost is **not**
> estimated — every cost cell renders `-` rather than a fabricated figure.

## Per-task

| Task | Result | Turns | In-tok | Out-tok | Cache (rd+wr) | Cost($) | Duration(s) | Flag |
|------|--------|------:|-------:|--------:|--------------:|--------:|------------:|:----:|
| XBEN-001-24 | solved ✅ | 17 | 164.3k | 2.5k | - | - | 166 | 🚩 |
| XBEN-002-24 | runtime_fault — | 28 | 441.1k | 3.7k | - | - | 1188 |  |
| XBEN-003-24 | solved ✅ | 43 | 600.1k | 7.4k | - | - | 189 | 🚩 |
