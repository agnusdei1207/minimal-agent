#!/usr/bin/env node
// Independent report generator for the Claude Code / XBOW-104 benchmark.
//
// Reads every artifacts/runs/<TASK>-<ts>/evidence.json for a model and rebuilds
//   benchmarks/claude/<model>/artifacts/reports/SUMMARY.md   (human report)
//   benchmarks/claude/<model>/artifacts/reports/kpi.json     (machine KPIs)
// with token / turn / cost metrics aggregated across ALL attempts (solved AND
// failed — this is total consumption, not just successes).
//
// It is deliberately standalone and READ-ONLY w.r.t. runs/: it never launches a
// solver, never touches docker, and never mutates evidence. Safe to run while a
// live `run.mjs` benchmark is in flight — it only re-reads the evidence already
// flushed to disk and rewrites the two report files. It does NOT overwrite
// results-index.json (that projection stays owned by run.mjs).
//
// Usage:
//   node benchmarks/claude/summarize.mjs                 # defaults to opus-4.8
//   node benchmarks/claude/summarize.mjs --model opus-4.8
//   node benchmarks/claude/summarize.mjs --model sonnet-5
//
// Intended workflow: the model owner runs this after a benchmark completes (or
// periodically while it runs) to refresh the rich SUMMARY.md + kpi.json from the
// evidence accumulated so far.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Mirror run.mjs's model registry so slugs/labels stay in sync.
const MODELS = {
  "opus-4.8": { id: "claude-opus-4-8", label: "Opus 4.8" },
  "sonnet-5": { id: "claude-sonnet-5", label: "Sonnet 5" },
  "haiku-4.5": { id: "claude-haiku-4-5", label: "Haiku 4.5" },
};

const arg = (name, dflt) => {
  const i = process.argv.indexOf(`--${name}`);
  if (i < 0) return dflt;
  const v = process.argv[i + 1];
  return v && !v.startsWith("--") ? v : true;
};

const MODEL_SLUG = String(arg("model", "opus-4.8"));
const MODEL = MODELS[MODEL_SLUG];
if (!MODEL) {
  console.error(
    `--model must be one of: ${Object.keys(MODELS).join(", ")} (got "${MODEL_SLUG}")`,
  );
  process.exit(2);
}

const MODEL_DIR = path.join(__dirname, MODEL_SLUG);
const RUNS_DIR = path.join(MODEL_DIR, "artifacts", "runs");
const REPORTS_DIR = path.join(MODEL_DIR, "artifacts", "reports");

// --------------------------------------------------------------------------
// Formatting helpers
// --------------------------------------------------------------------------
const num = (v) => (typeof v === "number" && Number.isFinite(v) ? v : 0);

// Compact human count: 34 -> "34", 5335 -> "5.3k", 1234567 -> "1.23M".
function kfmt(n) {
  n = num(n);
  if (Math.abs(n) < 1000) return String(n);
  if (Math.abs(n) < 1_000_000) return `${(n / 1000).toFixed(n % 1000 === 0 ? 0 : 1)}k`;
  return `${(n / 1_000_000).toFixed(2)}M`;
}
const usd = (n) => `$${num(n).toFixed(4)}`;
const usd2 = (n) => `$${num(n).toFixed(2)}`;
const secs = (n) => `${num(n)}s`;
function hms(totalS) {
  totalS = Math.round(num(totalS));
  const h = Math.floor(totalS / 3600);
  const m = Math.floor((totalS % 3600) / 60);
  const s = totalS % 60;
  return [h ? `${h}h` : null, m || h ? `${m}m` : null, `${s}s`]
    .filter(Boolean)
    .join(" ");
}
const div = (a, b) => (b ? a / b : 0);

// --------------------------------------------------------------------------
// Load newest valid evidence.json per task (mirrors run.mjs buildReports dedupe)
// --------------------------------------------------------------------------
function loadRows() {
  const byTask = new Map();
  if (!fs.existsSync(RUNS_DIR)) return [];
  for (const d of fs.readdirSync(RUNS_DIR)) {
    if (!d.startsWith("XBEN-")) continue;
    const evf = path.join(RUNS_DIR, d, "evidence.json");
    if (!fs.existsSync(evf)) continue; // in-progress run: no evidence yet
    let e;
    try {
      e = JSON.parse(fs.readFileSync(evf, "utf8"));
    } catch {
      continue; // skip malformed / partially-written
    }
    if (!e.task) continue;
    const prev = byTask.get(e.task);
    if (!prev || String(d) > String(prev._dir)) byTask.set(e.task, { ...e, _dir: d });
  }
  return [...byTask.values()].sort((a, b) => a.task.localeCompare(b.task));
}

// Per-row token extraction (usage may be null on faults).
function tok(row) {
  const u = row.usage || {};
  const input = num(u.input_tokens);
  const cache_creation = num(u.cache_creation_input_tokens);
  const cache_read = num(u.cache_read_input_tokens);
  const output = num(u.output_tokens);
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
// Build
// --------------------------------------------------------------------------
function build() {
  const rows = loadRows();
  const now = new Date().toISOString();

  const scoredRows = rows.filter((r) => r.valid_for_score !== false);
  const solved = rows.filter((r) => r.solved).length;
  const scored = scoredRows.length;
  const attempted = rows.length;
  const solveRate = scored ? (solved / scored) * 100 : 0;

  const tot = {
    input: 0,
    cache_creation: 0,
    cache_read: 0,
    output: 0,
    in_plus_out: 0,
    incl_cache: 0,
  };
  let totalTurns = 0;
  let totalWall = 0;
  let totalCost = 0;
  const perTask = rows.map((r) => {
    const t = tok(r);
    tot.input += t.input;
    tot.cache_creation += t.cache_creation;
    tot.cache_read += t.cache_read;
    tot.output += t.output;
    tot.in_plus_out += t.in_plus_out;
    tot.incl_cache += t.incl_cache;
    totalTurns += num(r.num_turns);
    totalWall += num(r.duration_s);
    totalCost += num(r.cost_usd);
    return { row: r, t };
  });

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

  const COST_NOTE =
    "예상 비용(API 종량제 요금 기준) — Estimated cost at standard API pricing";

  // ---- kpi.json ----
  const kpi = {
    model_slug: MODEL_SLUG,
    model_id: MODEL.id,
    model_label: MODEL.label,
    generated_at: now,
    generator: "benchmarks/claude/summarize.mjs",
    suite: "XBOW-104",
    solver: "claude-code-headless-host",
    cost_accounting: COST_NOTE,
    totals: {
      attempted,
      scored,
      solved,
      solve_rate_pct: Number(solveRate.toFixed(1)),
      wall_time_s: totalWall,
      note_infra_excluded: "wall_time_s is solver duration_s only; docker build/up/teardown excluded",
      tokens: tot,
      cost_usd: Number(totalCost.toFixed(6)),
    },
    averages: {
      per_task: {
        input_tokens: Math.round(avg.input_per_task),
        output_tokens: Math.round(avg.output_per_task),
        total_tokens_incl_cache: Math.round(avg.incl_cache_per_task),
        total_tokens_in_plus_out: Math.round(avg.in_plus_out_per_task),
        turns: Number(avg.turns_per_task.toFixed(1)),
        duration_s: Number(avg.duration_per_task.toFixed(1)),
        cost_usd: Number(avg.cost_per_task.toFixed(6)),
      },
      tokens_per_turn_incl_cache: Math.round(avg.tokens_per_turn_incl_cache),
      tokens_per_turn_in_plus_out: Math.round(avg.tokens_per_turn_in_plus_out),
    },
    tasks: perTask.map(({ row: r, t }) => ({
      task: r.task,
      name: r.name,
      level: r.level,
      outcome: r.outcome,
      solved: !!r.solved,
      valid_for_score: r.valid_for_score !== false,
      num_turns: r.num_turns ?? null,
      tokens: t,
      cost_usd: r.cost_usd ?? null,
      duration_s: r.duration_s ?? null,
      timed_out: !!r.timed_out,
      flag: r.flags_seen?.[0] ?? null,
    })),
  };

  fs.mkdirSync(REPORTS_DIR, { recursive: true });
  fs.writeFileSync(path.join(REPORTS_DIR, "kpi.json"), JSON.stringify(kpi, null, 2));

  // ---- SUMMARY.md ----
  const pct = solveRate.toFixed(1);
  const resultIcon = (r) => (r.solved ? "✅" : r.timed_out ? "⏱" : "—");
  const flagCell = (r) => (r.solved ? "🚩" : "");

  const lines = [
    `# Claude Code benchmark — ${MODEL.label} (\`${MODEL.id}\`)`,
    "",
    "Solver: Claude Code headless on host. Suite: XBOW-104.",
    `Regenerated: ${now} — by \`benchmarks/claude/summarize.mjs\``,
    "",
    "## Overview",
    "",
    `- Model: \`${MODEL.id}\` (${MODEL.label})`,
    `- Attempted: **${attempted}**`,
    `- Scored: **${scored}**`,
    `- SOLVED: **${solved}** (${pct}% of scored)`,
    `- Total solver wall time: **${hms(totalWall)}** (${totalWall}s) — infra (docker build/up/teardown) excluded`,
    "",
    "## Token totals — all attempts (solved + failed = total consumption)",
    "",
    "| Bucket | Tokens |",
    "|--------|-------:|",
    `| Input (fresh) | ${kfmt(tot.input)} |`,
    `| Cache write (creation) | ${kfmt(tot.cache_creation)} |`,
    `| Cache read | ${kfmt(tot.cache_read)} |`,
    `| Output | ${kfmt(tot.output)} |`,
    `| **Total (in + out)** | **${kfmt(tot.in_plus_out)}** |`,
    `| **Total incl. cache** | **${kfmt(tot.incl_cache)}** |`,
    "",
    "> Fresh `input` is tiny because prompt caching routes almost all context through",
    "> cache read/write; **Total incl. cache** is the true token throughput.",
    "",
    "## Averages",
    "",
    "| Metric | Value |",
    "|--------|------:|",
    `| Input / task | ${kfmt(Math.round(avg.input_per_task))} |`,
    `| Output / task | ${kfmt(Math.round(avg.output_per_task))} |`,
    `| Total tokens / task (incl. cache) | ${kfmt(Math.round(avg.incl_cache_per_task))} |`,
    `| Total tokens / task (in + out) | ${kfmt(Math.round(avg.in_plus_out_per_task))} |`,
    `| **Tokens / turn (incl. cache)** | ${kfmt(Math.round(avg.tokens_per_turn_incl_cache))} |`,
    `| Tokens / turn (in + out) | ${kfmt(Math.round(avg.tokens_per_turn_in_plus_out))} |`,
    `| Turns / task | ${avg.turns_per_task.toFixed(1)} |`,
    `| Duration / task | ${avg.duration_per_task.toFixed(0)}s |`,
    "",
    "## Cost",
    "",
    `> **${COST_NOTE}**`,
    "",
    "| Metric | Value |",
    "|--------|------:|",
    `| Estimated cost, total (API pricing) | ${usd2(totalCost)} |`,
    `| Estimated cost, per task avg (API pricing) | ${usd(avg.cost_per_task)} |`,
    "",
    "## Per-task",
    "",
    "| Task | Result | Turns | In-tok | Out-tok | Cache (rd+wr) | Cost($) | Duration(s) | Flag |",
    "|------|--------|------:|-------:|--------:|--------------:|--------:|------------:|:----:|",
    ...perTask.map(({ row: r, t }) => {
      const cache = t.cache_read + t.cache_creation;
      return (
        `| ${r.task} | ${r.outcome} ${resultIcon(r)} | ${r.num_turns ?? ""} | ` +
        `${kfmt(t.input)} | ${kfmt(t.output)} | ${kfmt(cache)} | ` +
        `${num(r.cost_usd).toFixed(4)} | ${r.duration_s ?? ""} | ${flagCell(r)} |`
      );
    }),
    "",
  ];

  fs.writeFileSync(path.join(REPORTS_DIR, "SUMMARY.md"), lines.join("\n"));

  console.log(
    `summarize: ${MODEL.label} — ${attempted} attempted, ${solved} solved (${pct}%), ` +
      `${kfmt(tot.incl_cache)} tok incl-cache, ${usd2(totalCost)} est. API cost.`,
  );
  console.log(`  wrote ${path.join(REPORTS_DIR, "SUMMARY.md")}`);
  console.log(`  wrote ${path.join(REPORTS_DIR, "kpi.json")}`);
}

build();
