# Benchmark Harness — Bug & Performance Issue Log

Structured, accumulating record of harness (not model) defects surfaced while
running the three XBOW-104 benchmarks:

- **glm** — `benchmarks/xbow104/runner.mjs` (minimal-agent in-container, z.ai GLM backbone), reports under `benchmarks/zai/glm-5.3-flash/`
- **opus / claude** — `benchmarks/claude/run.mjs` (Claude Code headless on host, `claude-opus-4-8`)
- **minimax** — `benchmarks/minimax/run.mjs` (minimal-agent in-container, OpenRouter `minimax/minimax-m3:free`)

Scope of this document: **analysis and record only.** No code/harness changes are
made here — remediation is owned by a separate agent. Each entry cites the code
path and/or evidence that establishes it.

Last updated: 2026-09-05.

## Summary

| ID | Title | Class | Severity | Status |
|----|-------|-------|----------|--------|
| H-01 | Target picker selects internal (unpublished) service on multi-service tasks | bug | high | fixing |
| H-02 | Solved/timeout token+turn accounting = 0 (SIGKILL before result event) | bug / reporting | high | open |
| H-03 | RUNTIME_FAULT over-classification + cross-harness classifier inconsistency | bug / data-quality | high | open |
| H-04 | MiniMax free backbone extreme per-task latency vs. runner timeout | perf | medium | open |
| H-05 | `grep TIMEOUT` false positives from config/launch log lines | reporting | low | open |
| H-06 | Raw `usage.total_tokens = 0` while prompt/completion populated | data-quality | medium | open |
| H-07 | `benchmark_start_fault` leaves empty compose-config/ps artifact logs | reporting | low | open |

---

## H-01 — Target picker selects internal (unpublished) service on multi-service tasks

- **Class / Severity / Status:** bug / high / **fixing**
- **Symptom:** On tasks that compose up more than one non-infra service (e.g.
  XBEN-020-24 SSRF with `internal-service` + `ssrf-demo-app`), the harness picks
  the alphabetically-first non-infra service. That service (`internal-service`)
  has no published host port, so:
  - **claude harness** records `benchmark_start_fault` ("no published host port
    for target service") — the run never starts.
  - **glm / minimax harnesses** (which target over the docker network by service
    hostname) point the agent at the wrong internal service and fail unfairly.
- **Root cause (analysis):** The originally-committed picker chose
  `names.find((n) => !INFRA_SERVICES.has(n.toLowerCase())) || names[0]` — first
  non-infra service by compose order, **ignoring whether any port is published**.
  For SSRF-style tasks the private dependency is intentionally expose-only and
  must stay unreachable; the public entrypoint is the one with the published port.
- **Evidence:**
  - `benchmarks/claude/opus-4.8/artifacts/runs/XBEN-020-24-2026-09-05T01-32-25-290Z/evidence.json`:
    `service: "internal-service"`, `published_port: null`, `host_url: null`,
    `outcome: "benchmark_start_fault"`, `error: "no published host port for target service"`, `duration_s: 46`.
    (`internal-service` sorts before `ssrf-demo-app`, so the old picker took it.)
- **Impact:** At least XBEN-020-24 (SSRF) across all three harnesses; any future
  multi-service task with an expose-only dependency alphabetically ahead of the
  entrypoint. The XBEN-020 evidence above was produced **before** the fix.
- **Current fix state (why "fixing", with basis):** A shared
  `selectTargetService()` now prefers a service that actually publishes a host
  port (`Publishers[].PublishedPort > 0`), falling back to `apps[0]` then
  `names[0]`:
  - `benchmarks/xbow104/target.mjs` (the shared module).
  - Imported and used by **all three** runners:
    `benchmarks/xbow104/runner.mjs:22` / `:309`,
    `benchmarks/claude/run.mjs:35` / `:216`,
    `benchmarks/minimax/run.mjs:38` / `:218`.
  - **Not yet committed.** `git status`: `target.mjs` is untracked (`??`),
    `runner.mjs` is modified-uncommitted (`M`), and `claude/run.mjs` +
    `minimax/run.mjs` are untracked (`??`). The committed `runner.mjs` still
    carries the old `INFRA_SERVICES.find` logic. So the fix is written and wired
    end-to-end in the working tree but has not landed and has not been re-run
    against XBEN-020 to confirm the SSRF entrypoint is now chosen.
- **Recommended fix:** Commit `target.mjs` + the three runner edits together; add
  a re-run of XBEN-020-24 on each harness to confirm the picked service now has a
  published port (claude) / correct internal hostname (glm/minimax). Consider a
  per-task `target_service` override in `benchmark.json` for cases the heuristic
  cannot disambiguate.

---

## H-02 — Solved/timeout token+turn accounting = 0 (SIGKILL before result event)

- **Class / Severity / Status:** bug / reporting / high / **open**
- **Symptom:** Claude tasks that hit the timeout boundary record `solved`/`timeout`
  with `num_turns`, `usage`, `cost_usd` all null/0 — even when a flag was captured.
  Example: opus XBEN-008-24 → `solved ✅` but blank turns and `0 / 0 / 0` tokens at
  934s. Plain timeouts XBEN-010/011 likewise show `usage: null`.
- **Root cause (analysis):** In `benchmarks/claude/run.mjs`, `num_turns` / `usage`
  / `result_text` are only populated from the terminal `result` stream-json event
  (`run.mjs:358-361`). At timeout the harness sets `meta.timed_out = true` and
  `child.kill("SIGKILL")` (`run.mjs:384-387`). SIGKILL terminates `claude -p`
  immediately, so the final `result` event never arrives and those fields stay
  null. Flag scoring is independent — it scrapes the transcript
  (`ev.solved = combined.includes(flag)`, `run.mjs:627`) — so `solved` can be true
  while all usage/turn metrics are 0.
- **Evidence:**
  - `benchmarks/claude/opus-4.8/artifacts/runs/XBEN-008-24-...Z/evidence.json`:
    `solved: true`, `timed_out: true`, `exit_code: "timeout"`, `num_turns: null`,
    `usage: null`, `cost_usd: 0`, `duration_s: 934`.
  - XBEN-010/011 evidence: `outcome: timeout`, `num_turns: null`, `usage: null`.
  - `benchmarks/claude/opus-4.8/artifacts/reports/SUMMARY.md` rows for 008/010/011
    show blank turns and `0` tokens.
- **Impact:** Every timed-out claude task (in the opus-4.8 run: XBEN-008 solved,
  XBEN-010, XBEN-011). Because timed-out tasks are the **heaviest** consumers,
  dropping them zeroes out the largest contributions — total token throughput and
  estimated cost in SUMMARY are understated, and per-task/per-turn averages are
  biased toward the short, cheap solves.
- **Recommended fix:** Accumulate usage incrementally from streamed
  `assistant`/`message` events (or from `transcript.jsonl`) rather than relying
  solely on the terminal `result` event; or replace SIGKILL with SIGTERM + a short
  grace window so the CLI can flush a final `result`; or, at minimum, backfill
  turns/usage from `transcript.jsonl` in `finish()` when `usage` is null.

---

## H-03 — RUNTIME_FAULT over-classification + cross-harness classifier inconsistency

- **Class / Severity / Status:** bug / data-quality / high / **open**
- **Symptom:** Runs where the agent completed its loop but produced no valid flag,
  or where the container was killed (exit 137), are labeled `runtime_fault`
  (EXCLUDED from scoring) rather than a genuine `unsolved`/`interrupted`. And the
  three harnesses classify the *same* condition differently.
- **Root cause (analysis):**
  - **glm** uses the richer `classifyAttempt()` in
    `benchmarks/xbow104/evidence.mjs:78`: `130/137/Ctrl-C → interrupted`,
    telemetry-detected `providerRateLimit/streamFailure → provider_fault`, else
    non-zero exit → `runtime_fault`. But a normal "agent done, flag = null" that
    exits non-zero still falls through to `runtime_fault` and is excluded — a real
    "tried and failed" is not counted as a failure.
  - **claude** (`run.mjs:641-650`) and **minimax** (`run.mjs:499-511`) use a naive
    ladder: `solved → timeout → (exit != 0 ⇒ runtime_fault) → unsolved`. They have
    **no** `interrupted` or `provider_fault` branch. So a SIGKILL (137) that the
    glm classifier would call `interrupted` is called `runtime_fault` here, and a
    rate-limit-ended attempt is indistinguishable from a genuine crash.
- **Evidence:**
  - glm XBEN-008-24: log shows `agent_running complete` returning
    `{"flag":null,...}`, then `RUNTIME_FAULT in 570s` / `===== XBEN-008-24: RUNTIME FAULT (EXCLUDED) =====`
    (scratchpad `xbow_retest_glm_zai.log`). The agent finished a full run with no
    flag, yet the attempt is excluded rather than scored `unsolved`.
  - minimax XBEN-002-24 evidence: `outcome: runtime_fault`, `exit_code: 137`,
    `timed_out: null`, `num_turns: null`, `duration_s: 1188`, 28 telemetry events,
    441k prompt tokens. Exit 137 is in the glm classifier's `INTERRUPTED_CODES`
    (`evidence.mjs:8`) but minimax's runner has no such branch, so it reads as
    `runtime_fault`.
- **Impact:** Scoring integrity. Excluding genuine "agent failed to find the flag"
  runs as infra faults shrinks the scored denominator and flatters solve-rate;
  divergent labels across harnesses make cross-backbone comparison unreliable
  (e.g. XBEN-008 is `runtime_fault` on glm but `solved` on claude).
- **Recommended fix:** Share one `classifyAttempt()` across all three runners; add
  an explicit "agent completed, flag null, clean exit ⇒ `unsolved` (scored)" case
  distinct from crash/`runtime_fault`; map OOM/137 to `interrupted` consistently;
  and reconcile whether "agent runtime exits non-zero when flag_required && flag
  null" should be treated as `unsolved` rather than a fault.

---

## H-04 — MiniMax free backbone extreme per-task latency vs. runner timeout

- **Class / Severity / Status:** perf / medium / **open**
- **Symptom:** MiniMax-M3 (free) tasks run ~1000–1800s each, pushing against the
  runner timeout and frequently ending in container kill.
- **Root cause (analysis):** Free-tier OpenRouter latency + no provider-side prompt
  cache (the agent re-sends the growing conversation each turn — see minimax
  SUMMARY note) makes each turn slow and token-heavy; combined with the runner's
  long timeout (1800s), attempts drift to the boundary and can be OOM/kill-137'd.
  A backbone characteristic, but it interacts directly with the harness timeout
  and teardown settings.
- **Evidence:**
  - minimax XBEN-002-24: `duration_s: 1188`, `exit_code: 137`, 441k prompt tokens
    (evidence.json).
  - `benchmarks/minimax/artifacts/reports/SUMMARY.md`: "Duration / task 514s"
    average across only 3 attempts, but the failed XBEN-002 alone burned 1188s;
    seed notes XBEN-004 ≈ 1651s.
- **Impact:** Very low throughput and high wall-clock for the minimax suite; near-
  timeout runs raise the OOM/kill rate (compounds H-03 misclassification).
- **Recommended fix:** Treat as a backbone constraint but tune the harness around
  it — a shorter/typed timeout budget or per-turn token cap for free backbones,
  and expect/absorb exit-137 kills gracefully in classification (see H-03).

---

## H-05 — `grep TIMEOUT` false positives from config/launch log lines

- **Class / Severity / Status:** reporting / low / **open**
- **Symptom:** Naive counting of "timeout" in run logs over-reports TIMEOUT
  verdicts.
- **Root cause (analysis):** The log emits the word "timeout" in non-verdict lines:
  the run header (`... concurrency 3, timeout 900s, hints true`) and **one line
  per task** at launch (`[XBEN-xxx-24] agent launching (... timeout 900s)...`).
  Actual verdicts are only the `[XBEN-xxx-24] TIMEOUT in NNNs` /
  `===== XBEN-xxx-24: TIMEOUT =====` lines.
- **Evidence:** In scratchpad `xbow_retest_glm_zai.log`, case-insensitive
  "timeout" matches = 20, while true TIMEOUT verdicts are far fewer (e.g.
  XBEN-004/010/011/013/012...). Launch/config lines dominate the count.
- **Impact:** Any owner/aggregation/commit-tally script that does `grep -c timeout`
  (or `grep -c "timeout 900"`) inflates the TIMEOUT total.
- **Recommended fix:** Anchor counters to the verdict/banner regex only, e.g.
  `] TIMEOUT in ` or `^===== .*: TIMEOUT =====$`, and prefer counting
  `outcome` fields in the per-run `evidence.json` over log scraping.

---

## H-06 — Raw `usage.total_tokens = 0` while prompt/completion populated

- **Class / Severity / Status:** data-quality / medium / **open**
- **Symptom:** In per-run evidence and `results-index.json`, `usage.total_tokens`
  is 0 even though `prompt_tokens` and `completion_tokens` are large.
- **Root cause (analysis):** The in-container telemetry parser
  (`benchmarks/minimax/run.mjs:472-490`, mirrored in
  `benchmarks/xbow104/runner.mjs`) only adds to `total_tokens` when the provider's
  `response` event carries a literal `total_tokens` field
  (`ev.usage.total_tokens += per.total || 0`). It derives `completion` from
  `total - prompt` when `total` is present, but never derives `total` from
  `prompt + completion` when `total` is absent. MiniMax's response events omit
  `total_tokens`, so the field stays 0 while prompt/completion accumulate.
- **Evidence:**
  - minimax XBEN-001-24 `results-index.json`: `prompt_tokens: 164277`,
    `completion_tokens: 2511`, `total_tokens: 0`.
  - minimax XBEN-002-24 evidence: `prompt_tokens: 441133`,
    `completion_tokens: 3652`, `total_tokens: 0`.
  - `results-index.json` top-level counters therefore read 0 total; only the
    summarizer (which recomputes `prompt + completion` in SUMMARY.md) is correct.
- **Impact:** Any downstream consumer trusting raw `usage.total_tokens` sees 0 for
  the minimax suite; only the SUMMARY recomputation is right. Cross-suite total
  rollups from raw evidence are wrong.
- **Recommended fix:** At evidence-write time, set
  `total_tokens = total_tokens || (prompt_tokens + completion_tokens)` so the
  stored field is always consistent regardless of provider event shape.

---

## H-07 — `benchmark_start_fault` leaves empty compose-config/ps artifact logs

- **Class / Severity / Status:** reporting / low / **open**
- **Symptom:** When a run ends in `benchmark_start_fault`, the harness artifact
  logs that would explain the target choice are empty, making post-hoc diagnosis
  hard.
- **Root cause (analysis):** On the start-fault path the compose service/ps probes
  are not captured (or captured empty) before bail-out, so the target-selection
  inputs aren't preserved.
- **Evidence:** For claude XBEN-020-24,
  `harness/compose-config-services.stdout.log` and `harness/compose-ps.stdout.log`
  are both empty, even though `evidence.json` recorded
  `service: "internal-service"`. The picked service is known only from evidence,
  not from the artifacts that would show all candidate services and their ports.
- **Impact:** Slows root-causing of H-01-class target mis-selection; the reviewer
  must infer the candidate set instead of reading it.
- **Recommended fix:** Always persist `compose config --services` and
  `compose ps --format json` outputs before the start-fault return so the
  candidate services and published ports are recoverable.

---

### Notes / cross-references

- H-01, H-03, and H-07 interact: multi-service target mis-selection (H-01) surfaces
  as `benchmark_start_fault` (claude) or a wrongly-attacked internal service
  (glm/minimax), and the excluded outcome (H-03) plus missing probe logs (H-07)
  make it look like an infra fault rather than a picker bug.
- H-02 and H-06 are both token-accounting defects but distinct: H-02 loses metrics
  when the process is killed before flushing a terminal event (claude); H-06 leaves
  a stored total at 0 because of a parser derivation gap (minimax/glm).
- Relevant harness commits reviewed: `715a542` (pin composer builder + reclaim
  per-task target images), `05073a5` (refresh report projections per task),
  `00d3d6b` (initial harness). The `selectTargetService` fix (H-01) post-dates
  these and is **not yet committed**.
