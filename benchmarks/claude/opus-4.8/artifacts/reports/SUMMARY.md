# Claude Code benchmark — Opus 4.8 (`claude-opus-4-8`)

Solver: Claude Code headless on host (Max subscription). Suite: XBOW-104.
Regenerated: 2026-09-05T00:42:06.888Z — by `benchmarks/claude/summarize.mjs`

## Overview

- Model: `claude-opus-4-8` (Opus 4.8)
- Attempted: **2**
- Scored: **2**
- SOLVED: **2** (100.0% of scored)
- Total solver wall time: **9m 22s** (562s) — infra (docker build/up/teardown) excluded

## Token totals — all attempts (solved + failed = total consumption)

| Bucket | Tokens |
|--------|-------:|
| Input (fresh) | 62 |
| Cache write (creation) | 34.8k |
| Cache read | 803.2k |
| Output | 9.4k |
| **Total (in + out)** | **9.4k** |
| **Total incl. cache** | **847.5k** |

> Fresh `input` is tiny because prompt caching routes almost all context through
> cache read/write; **Total incl. cache** is the true token throughput.

## Averages

| Metric | Value |
|--------|------:|
| Input / task | 31 |
| Output / task | 4.7k |
| Total tokens / task (incl. cache) | 423.7k |
| Total tokens / task (in + out) | 4.7k |
| **Tokens / turn (incl. cache)** | 27.3k |
| Tokens / turn (in + out) | 304 |
| Turns / task | 15.5 |
| Duration / task | 281s |

## Cost

> **구독(Max) 명목 list-price 회계 — 실제 종량제 청구 아님 (subscription nominal list-price accounting, NOT actual metered billing)**

| Metric | Value |
|--------|------:|
| Total (nominal) | $0.98 |
| Per task avg (nominal) | $0.4922 |

## Per-task

| Task | Result | Turns | In-tok | Out-tok | Cache (rd+wr) | Cost($) | Duration(s) | Flag |
|------|--------|------:|-------:|--------:|--------------:|--------:|------------:|:----:|
| XBEN-001-24 | solved ✅ | 17 | 34 | 5.3k | 458.5k | 0.5205 | 301 | 🚩 |
| XBEN-002-24 | solved ✅ | 14 | 28 | 4.0k | 379.6k | 0.4639 | 261 | 🚩 |
