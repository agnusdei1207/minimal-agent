#!/usr/bin/env node
// Shared standard benchmark report renderer.
//
// One renderer, one format — used by every model family so their SUMMARY.md /
// kpi.json line up column-for-column:
//   - pentesting (glm) (benchmarks/zai/summarize.mjs)         — in-container
//
// It is READ-ONLY w.r.t. runs/: it only re-reads evidence.json / telemetry
// already flushed to disk and rewrites the two report files. It never launches a
// solver, never touches docker, never mutates evidence. Safe to run while a live
// benchmark is in flight.
//
// The per-task table columns are identical across every family:
//   | Task | Result | Turns | In-tok | Out-tok | Cache (rd+wr) | Cost($) | Duration(s) | Flag |
//
// Callers adapt their raw evidence into normalized rows (see NormRow below) and
// hand them to renderStandardReport(); the aggregation + markdown/JSON emission
// is shared here so the numbers and layout cannot drift between families.

import fs from "node:fs";
import path from "node:path";

// --------------------------------------------------------------------------
// Formatting helpers (shared, exported so callers can reuse if needed)
// --------------------------------------------------------------------------
export const num = (v) => (typeof v === "number" && Number.isFinite(v) ? v : 0);

// Compact human count: 34 -> "34", 5335 -> "5.3k", 1234567 -> "1.23M".
export function kfmt(n) {
  n = num(n);
  if (Math.abs(n) < 1000) return String(n);
  if (Math.abs(n) < 1_000_000) return `${(n / 1000).toFixed(n % 1000 === 0 ? 0 : 1)}k`;
  return `${(n / 1_000_000).toFixed(2)}M`;
}
export const usd = (n) => `$${num(n).toFixed(4)}`;
export const usd2 = (n) => `$${num(n).toFixed(2)}`;
export function hms(totalS) {
  totalS = Math.round(num(totalS));
  const h = Math.floor(totalS / 3600);
  const m = Math.floor((totalS % 3600) / 60);
  const s = totalS % 60;
  return [h ? `${h}h` : null, m || h ? `${m}m` : null, `${s}s`].filter(Boolean).join(" ");
}
const div = (a, b) => (b ? a / b : 0);

// Cost accounting note — memory rule: estimate at standard API metered pricing
// ONLY; never "subscription"/"nominal"/"actually billed". When per-token pricing
// is unknown the cost cells render "-" (never fabricated).
export const COST_NOTE =
  "예상 비용(API 종량제 요금 기준) — Estimated cost at standard API pricing";

// --------------------------------------------------------------------------
// Normalized row (NormRow) — what every caller must produce per task:
//   {
//     task, name, level,
//     outcome, solved, valid_for_score, timed_out,
//     num_turns,        // number | null
//     input, cache_creation, cache_read, output,  // token counts (0 if n/a)
//     cache_known,      // bool: are cache figures provider-reported & meaningful?
//     cost_usd,         // number | null  (null => unit price unknown => "-")
//     duration_s,       // number | null: recorded attempt elapsed, includes setup;
//                       // teardown inclusion varies by harness, not solver latency
//     flag,             // string | null
//   }
// --------------------------------------------------------------------------

function rowTokens(r) {
  const input = num(r.input);
  const cache_creation = num(r.cache_creation);
  const cache_read = num(r.cache_read);
  const output = num(r.output);
  return {
    input,
    cache_creation,
    cache_read,
    output,
    in_plus_out: input + output,
    incl_cache: input + cache_creation + cache_read + output,
  };
}

// --------------------------------------------------------------------------
// renderStandardReport(opts)
//   model:   { slug, id, label }
//   rows:    NormRow[]  (already deduped to newest-per-task by the caller)
//   reportsDir: absolute path to write SUMMARY.md + kpi.json
//   generator:  string identifying the calling script (provenance)
//   header:  { title, solverLine }  markdown header + one-line solver descriptor
//   suite:   default "XBOW-104"
//   solver:  short solver id for kpi.json
//   costKnown: force cost accounting on/off (default: inferred from rows)
// Returns a small stats object for the caller to log.
// --------------------------------------------------------------------------
export function renderStandardReport(opts) {
  const {
    model,
    rows,
    reportsDir,
    generator,
    header,
    suite = "XBOW-104",
    solver = "unknown",
  } = opts;

  const now = new Date().toISOString();
  const sorted = [...rows].sort((a, b) => String(a.task).localeCompare(String(b.task)));
  const consumption = opts.attempts ?? sorted;

  const scoredRows = sorted.filter((r) => r.valid_for_score === true);
  const solved = scoredRows.filter((r) => r.solved).length;
  const scored = scoredRows.length;
  const SUITE_TOTAL = 104; // XBOW-104 suite total — denominator is always 104
  const attempted = sorted.length;
  const solveRate = (solved / SUITE_TOTAL) * 100;

  // A complete cost total requires a measurement for every retained attempt.
  // An absent measurement must not silently become a zero-dollar attempt.
  const cacheKnown = consumption.some((r) => r.cache_known);
  const costKnown =
    opts.costKnown ?? (consumption.length > 0 && consumption.every((r) => r.cost_usd != null && Number.isFinite(r.cost_usd)));

  const tot = { input: 0, cache_creation: 0, cache_read: 0, output: 0, in_plus_out: 0, incl_cache: 0 };
  let totalTurns = 0;
  let totalWall = 0;
  let totalCost = 0;
  const turnsKnown = consumption.length > 0 && consumption.every((r) => Number.isFinite(r.num_turns));
  for (const r of consumption) {
    const t = rowTokens(r);
    tot.input += t.input;
    tot.cache_creation += t.cache_creation;
    tot.cache_read += t.cache_read;
    tot.output += t.output;
    tot.in_plus_out += t.in_plus_out;
    tot.incl_cache += t.incl_cache;
    totalTurns += num(r.num_turns);
    totalWall += num(r.duration_s);
    if (r.cost_usd != null && Number.isFinite(r.cost_usd)) totalCost += r.cost_usd;
  }
  const perTask = sorted.map((r) => ({ row: r, t: rowTokens(r) }));

  const avg = {
    input_per_task: div(tot.input, attempted),
    output_per_task: div(tot.output, attempted),
    incl_cache_per_task: div(tot.incl_cache, attempted),
    in_plus_out_per_task: div(tot.in_plus_out, attempted),
    turns_per_task: div(totalTurns, attempted),
    duration_per_task: div(totalWall, attempted),
    cost_per_task: div(totalCost, attempted),
    tokens_per_turn_incl_cache: div(tot.incl_cache, totalTurns),
    tokens_per_turn_in_plus_out: div(tot.in_plus_out, totalTurns),
  };

  // ------------------------------------------------------------------ kpi.json
  const kpi = {
    model_slug: model.slug,
    model_id: model.id,
    model_label: model.label,
    generated_at: now,
    generator,
    suite,
    solver,
    cache_tracked: cacheKnown,
    cost_accounting: costKnown ? COST_NOTE : "cost unavailable or incomplete — missing price or attempt measurement; aggregate cost fields are null/\"-\"",
    totals: {
      suite_total: SUITE_TOTAL,
      attempted,
      not_attempted: SUITE_TOTAL - attempted,
      attempt_count: consumption.length,
      selection_policy: "newest finalized attempt per task",
      excluded: attempted - scored,
      scored,
      solved,
      solve_rate_pct: Number(solveRate.toFixed(1)),
      wall_time_s: totalWall,
      // Retain the legacy JSON key; its value corrects the old exclusion claim.
      note_infra_excluded:
        "wall_time_s sums recorded attempt duration_s; includes setup; teardown inclusion varies by harness. This is not solver-only time or per-turn latency.",
      tokens: tot,
      cost_usd: costKnown ? Number(totalCost.toFixed(6)) : null,
    },
    attempts: consumption.map((r) => ({
      task: r.task, stamp: r.stamp ?? null, outcome: r.outcome ?? null,
      valid_for_score: r.valid_for_score === true, solved: !!r.solved,
      tokens: rowTokens(r), num_turns: r.num_turns ?? null,
      cost_usd: r.cost_usd ?? null, duration_s: r.duration_s ?? null,
    })),
    averages: {
      per_task: {
        input_tokens: Math.round(avg.input_per_task),
        output_tokens: Math.round(avg.output_per_task),
        total_tokens_incl_cache: Math.round(avg.incl_cache_per_task),
        total_tokens_in_plus_out: Math.round(avg.in_plus_out_per_task),
        turns: turnsKnown ? Number(avg.turns_per_task.toFixed(1)) : null,
        duration_s: Number(avg.duration_per_task.toFixed(1)),
        cost_usd: costKnown ? Number(avg.cost_per_task.toFixed(6)) : null,
      },
      tokens_per_turn_incl_cache: turnsKnown && totalTurns > 0 ? Math.round(avg.tokens_per_turn_incl_cache) : null,
      tokens_per_turn_in_plus_out: turnsKnown && totalTurns > 0 ? Math.round(avg.tokens_per_turn_in_plus_out) : null,
    },
    tasks: perTask.map(({ row: r, t }) => ({
      task: r.task,
      stamp: r.stamp ?? null,
      name: r.name ?? null,
      level: r.level ?? null,
      outcome: r.outcome ?? null,
      solved: !!r.solved,
      valid_for_score: r.valid_for_score === true,
      num_turns: r.num_turns ?? null,
      tokens: t,
      cache_known: !!r.cache_known,
      cost_usd: r.cost_usd ?? null,
      duration_s: r.duration_s ?? null,
      timed_out: !!r.timed_out,
      flag: r.flag ?? null,
    })),
  };

  fs.mkdirSync(reportsDir, { recursive: true });
  fs.writeFileSync(path.join(reportsDir, "kpi.json"), JSON.stringify(kpi, null, 2));

  // ----------------------------------------------------------------- SUMMARY.md
  const pct = solveRate.toFixed(1);
  const resultIcon = (r) => (r.solved ? "✅" : r.timed_out ? "⏱" : "—");
  const flagCell = (r) => (r.solved ? "🚩" : "");
  const cacheCell = (t) => (cacheKnown ? kfmt(t.cache_read + t.cache_creation) : "-");
  const costCell = (r) =>
    costKnown && r.cost_usd != null && Number.isFinite(r.cost_usd)
      ? num(r.cost_usd).toFixed(4)
      : "-";

  const tokenTotalsRows = cacheKnown
    ? [
        `| Input (fresh) | ${kfmt(tot.input)} |`,
        `| Cache write (creation) | ${kfmt(tot.cache_creation)} |`,
        `| Cache read | ${kfmt(tot.cache_read)} |`,
        `| Output | ${kfmt(tot.output)} |`,
        `| **Total (in + out)** | **${kfmt(tot.in_plus_out)}** |`,
        `| **Total incl. cache** | **${kfmt(tot.incl_cache)}** |`,
      ]
    : [
        `| Input (prompt) | ${kfmt(tot.input)} |`,
        `| Output (completion) | ${kfmt(tot.output)} |`,
        `| **Total (in + out)** | **${kfmt(tot.in_plus_out)}** |`,
      ];

  const tokenNote = cacheKnown
    ? [
        "> Fresh `input` is tiny because API prompt caching routes almost all context through",
        "> cache read/write; **Total incl. cache** is the true token throughput.",
      ]
    : [];

  const costSection = costKnown
    ? [
        "## Cost",
        "",
        `> **${COST_NOTE}**`,
        "",
        "| Metric | Value |",
        "|--------|------:|",
        `| Estimated cost, total (API pricing) | ${usd2(totalCost)} |`,
        `| Estimated cost, per task avg (API pricing) | ${usd(avg.cost_per_task)} |`,
        "",
      ]
    : [
        "## Cost",
        "",
        "> Complete cost is unavailable: a unit price or an attempt measurement is missing.",
        "> Aggregate cost renders `-`; raw per-attempt measurements remain in kpi.json.",
        "",
      ];

  const perTaskHeader = cacheKnown
    ? [
        "| Task | Result | Turns | In-tok | Out-tok | Prompt Cache | Cost($) | Duration(s) | Flag |",
        "|------|--------|------:|-------:|--------:|-------------:|--------:|------------:|:----:|",
      ]
    : [
        "| Task | Result | Turns | In-tok | Out-tok | Cost($) | Duration(s) | Flag |",
        "|------|--------|------:|-------:|--------:|--------:|------------:|:----:|",
      ];

  const perTaskRows = perTask.map(({ row: r, t }) => {
    if (cacheKnown) {
      return (
        `| ${r.task} | ${r.outcome} ${resultIcon(r)} | ${r.num_turns ?? ""} | ` +
        `${kfmt(t.input)} | ${kfmt(t.output)} | ${cacheCell(t)} | ` +
        `${costCell(r)} | ${r.duration_s ?? ""} | ${flagCell(r)} |`
      );
    } else {
      return (
        `| ${r.task} | ${r.outcome} ${resultIcon(r)} | ${r.num_turns ?? ""} | ` +
        `${kfmt(t.input)} | ${kfmt(t.output)} | ` +
        `${costCell(r)} | ${r.duration_s ?? ""} | ${flagCell(r)} |`
      );
    }
  });

  const lines = [
    header.title,
    "",
    header.solverLine,
    `Regenerated: ${now} — by \`${generator}\``,
    "",
    "## Overview",
    "",
    `- Suite total: **${SUITE_TOTAL}**`,
    `- Attempted: **${attempted}** (not attempted: **${SUITE_TOTAL - attempted}**)`,
    `- Retained finalized attempts (including retries): **${consumption.length}**`,
    "- Score selection: newest finalized attempt per task. Consumption includes all retained finalized attempts; previously deleted evidence cannot be reconstructed.",
    `- Scored: **${scored}**`,
    `- SOLVED: **${solved} / ${SUITE_TOTAL}** (${pct}%)`,
    `- Recorded attempt elapsed time: **${hms(totalWall)}** (${totalWall}s) — includes setup; teardown inclusion varies by harness`,
    "- Recorded durations are summed across retained attempts, including concurrent attempts; they are not campaign wall time or solver-only latency.",
    "",
    "## Token totals — all attempts (solved + failed = total consumption)",
    "",
    "| Bucket | Tokens |",
    "|--------|-------:|",
    ...tokenTotalsRows,
    "",
    ...tokenNote,
    ...(tokenNote.length > 0 ? [""] : []),
    "## Averages",
    "",
    "| Metric | Value |",
    "|--------|------:|",
    `| Input / task | ${kfmt(Math.round(avg.input_per_task))} |`,
    `| Output / task | ${kfmt(Math.round(avg.output_per_task))} |`,
    `| Total tokens / task (in + out) | ${kfmt(Math.round(avg.in_plus_out_per_task))} |`,
    `| **Tokens / turn (in + out)** | ${turnsKnown && totalTurns > 0 ? kfmt(Math.round(avg.tokens_per_turn_in_plus_out)) : "-"} |`,
    `| Turns / task | ${turnsKnown ? avg.turns_per_task.toFixed(1) : "-"} |`,
    `| Duration / task | ${avg.duration_per_task.toFixed(0)}s |`,
    "",
    ...costSection,
    "## Per-task",
    "",
    ...perTaskHeader,
    ...perTaskRows,
    "",
  ];

  fs.writeFileSync(path.join(reportsDir, "SUMMARY.md"), lines.join("\n"));

  return { attempted, scored, solved, solveRate, tot, totalCost, cacheKnown, costKnown, reportsDir };
}

// --------------------------------------------------------------------------
// Adapter: pentesting evidence layout (native runtime) -> NormRow[]
//
// Per task artifacts/runs/<TASK>-<ts>/:
//   evidence.json          -> outcome/solved/duration_s/flags_seen/valid_for_score
//   telemetry/usage.jsonl  -> one line per model response
//                             {prompt_tokens, completion_tokens, (opt) cached_tokens}
//
//   Turns   = number of response lines
//   In-tok  = Σ prompt_tokens
//   Out-tok = Σ completion_tokens
//   Cache   = Σ cached_tokens if any line reports it, else unknown ("-")
//   Duration/Flag/outcome from evidence.json
//
// Falls back to evidence.usage {prompt_tokens, completion_tokens, cached_tokens,
// events} when telemetry is missing/empty. Dedupes to newest ts per task.
// --------------------------------------------------------------------------
export function loadMinimalAgentRows(runsDir, { allAttempts = false } = {}) {
  const byTask = new Map();
  if (!fs.existsSync(runsDir)) return [];
  for (const d of fs.readdirSync(runsDir)) {
    if (!d.startsWith("XBEN-")) continue;
    const dir = path.join(runsDir, d);
    const evf = path.join(dir, "evidence.json");
    if (!fs.existsSync(evf)) continue; // in-progress: no evidence yet
    let e;
    try {
      e = JSON.parse(fs.readFileSync(evf, "utf8"));
    } catch {
      continue;
    }
    if (!e.task) continue;
    const key = allAttempts ? d : e.task;
    const prev = byTask.get(key);
    if (!prev || String(d) > String(prev._dir)) byTask.set(key, { ...e, _dir: d, _dir_full: dir });
  }

  return [...byTask.values()].map((e) => {
    // Aggregate telemetry.
    let input = 0;
    let output = 0;
    let cached = 0;
    let cacheSeen = false;
    let turns = 0;
    const tf = path.join(e._dir_full, "telemetry", "usage.jsonl");
    if (fs.existsSync(tf)) {
      const text = fs.readFileSync(tf, "utf8");
      for (const line of text.split(/\r?\n/)) {
        const s = line.trim();
        if (!s) continue;
        let j;
        try {
          j = JSON.parse(s);
        } catch {
          continue;
        }
        if (j.event !== "response" && (j.event ||
          (!Number.isFinite(j.prompt_tokens) && !Number.isFinite(j.completion_tokens)))) continue;
        turns += 1;
        input += num(j.prompt_tokens);
        output += num(j.completion_tokens);
        if (j.cached_tokens != null) {
          cacheSeen = true;
          cached += num(j.cached_tokens);
        }
      }
    }
    // Fallback to evidence.usage when telemetry is absent/empty.
    if (turns === 0 && e.usage) {
      input = num(e.usage.prompt_tokens);
      output = num(e.usage.completion_tokens);
      turns = num(e.usage.events);
      if (e.usage.cached_tokens != null) {
        cached = num(e.usage.cached_tokens);
        // treat as known only if positive (these runs report 0 == "not tracked")
        cacheSeen = cached > 0;
      }
    }
    const cache_known = cacheSeen && cached > 0;

    return {
      task: e.task,
      stamp: e._dir,
      name: e.name ?? null,
      level: e.level ?? null,
      outcome: e.outcome ?? (e.solved ? "solved" : e.timed_out ? "timeout" : "failed"),
      solved: !!e.solved,
      valid_for_score: e.valid_for_score === true,
      timed_out: !!e.timed_out,
      num_turns: turns || null,
      // OpenAI-style prompt_tokens already includes cached prompt tokens.
      input: Math.max(0, input - (cache_known ? cached : 0)),
      cache_creation: 0,
      cache_read: cache_known ? cached : 0,
      output,
      cache_known,
      cost_usd: null, // no established per-token price -> "-"
      duration_s: e.duration_s ?? null,
      flag: e.flags_seen?.[0] ?? null,
    };
  });
}
