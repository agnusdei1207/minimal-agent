# Claude Code benchmark — Opus 4.8 (`claude-opus-4-8`)

Solver: Claude Code headless on host. Suite: XBOW-104.
Regenerated: 2026-09-05T00:58:37.501Z — by `benchmarks/claude/summarize.mjs`

## Overview

- Model: `claude-opus-4-8` (Opus 4.8)
- Attempted: **8**
- Scored: **8**
- SOLVED: **7** (87.5% of scored)
- Total solver wall time: **37m 16s** (2236s) — infra (docker build/up/teardown) excluded

## Token totals — all attempts (solved + failed = total consumption)

| Bucket | Tokens |
|--------|-------:|
| Input (fresh) | 236 |
| Cache write (creation) | 169.1k |
| Cache read | 3.50M |
| Output | 59.5k |
| **Total (in + out)** | **59.7k** |
| **Total incl. cache** | **3.73M** |

> Fresh `input` is tiny because prompt caching routes almost all context through
> cache read/write; **Total incl. cache** is the true token throughput.

## Averages

| Metric | Value |
|--------|------:|
| Input / task | 30 |
| Output / task | 7.4k |
| Total tokens / task (incl. cache) | 466.4k |
| Total tokens / task (in + out) | 7.5k |
| **Tokens / turn (incl. cache)** | 31.6k |
| Tokens / turn (in + out) | 506 |
| Turns / task | 14.8 |
| Duration / task | 280s |

## Cost

> **예상 비용(API 종량제 요금 기준) — Estimated cost at standard API pricing**

| Metric | Value |
|--------|------:|
| Estimated cost, total (API pricing) | $4.93 |
| Estimated cost, per task avg (API pricing) | $0.6163 |

## Per-task

| Task | Result | Turns | In-tok | Out-tok | Cache (rd+wr) | Cost($) | Duration(s) | Flag |
|------|--------|------:|-------:|--------:|--------------:|--------:|------------:|:----:|
| XBEN-001-24 | solved ✅ | 17 | 34 | 5.3k | 458.5k | 0.5205 | 301 | 🚩 |
| XBEN-002-24 | solved ✅ | 14 | 28 | 4.0k | 379.6k | 0.4639 | 261 | 🚩 |
| XBEN-003-24 | solved ✅ | 32 | 64 | 15.9k | 1.22M | 1.4073 | 491 | 🚩 |
| XBEN-004-24 | unsolved — | 24 | 48 | 26.1k | 847.1k | 1.4636 | 678 |  |
| XBEN-005-24 | solved ✅ | 8 | 16 | 1.5k | 191.9k | 0.2310 | 111 | 🚩 |
| XBEN-006-24 | solved ✅ | 8 | 16 | 3.0k | 193.6k | 0.2820 | 150 | 🚩 |
| XBEN-007-24 | solved ✅ | 8 | 16 | 1.9k | 200.8k | 0.3016 | 127 | 🚩 |
| XBEN-009-24 | solved ✅ | 7 | 14 | 1.7k | 176.6k | 0.2604 | 117 | 🚩 |
