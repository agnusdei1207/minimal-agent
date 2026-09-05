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

import { renderStandardReport, num, kfmt, usd2 } from '../lib/standard-report.mjs';

// Keep every finalized attempt for consumption and provenance. Score rows use
// the newest finalized attempt per task, matching the other provider reports.
const attempts = [];
const latest = new Map();
if (fs.existsSync(RUNS_DIR)) {
  for (const stamp of fs.readdirSync(RUNS_DIR).sort()) {
    if (!stamp.startsWith('XBEN-')) continue;
    const file = path.join(RUNS_DIR, stamp, 'evidence.json');
    let evidence;
    try { evidence = JSON.parse(fs.readFileSync(file, 'utf8')); }
    catch { continue; }
    if (!evidence.task) continue;
    const usage = evidence.usage || {};
    const row = {
      task: evidence.task, stamp, name: evidence.name, level: evidence.level,
      outcome: evidence.outcome ?? (evidence.solved ? 'solved' : 'unsolved'),
      solved: !!evidence.solved, valid_for_score: evidence.valid_for_score === true,
      timed_out: !!evidence.timed_out, num_turns: evidence.num_turns ?? null,
      input: num(usage.input_tokens), output: num(usage.output_tokens),
      cache_creation: num(usage.cache_creation_input_tokens),
      cache_read: num(usage.cache_read_input_tokens),
      cache_known: usage.cache_creation_input_tokens != null || usage.cache_read_input_tokens != null,
      cost_usd: evidence.cost_usd ?? null, duration_s: evidence.duration_s ?? null,
      flag: evidence.flags_seen?.[0] ?? null,
    };
    attempts.push(row);
    latest.set(row.task, row);
  }
}
const result = renderStandardReport({
  model: { slug: MODEL_SLUG, id: MODEL.id, label: MODEL.label },
  rows: [...latest.values()], attempts, reportsDir: REPORTS_DIR,
  generator: 'benchmarks/claude/summarize.mjs', suite: 'XBOW-104',
  solver: 'claude-code-headless-host',
  header: {
    title: `# Claude Code benchmark — ${MODEL.label} (${MODEL.id})`,
    solverLine: 'Solver: Claude Code headless on host. Suite: XBOW-104.',
  },
});
console.log(`summarize: ${MODEL.label}: ${result.solved}/${result.scored} solved (${result.solveRate.toFixed(1)}%), ${kfmt(result.tot.incl_cache)} tokens; cost ${result.costKnown ? usd2(result.totalCost) : 'unavailable'}.`);