#!/usr/bin/env node
// Standard report generator for the MiniMax minimal-agent benchmark.
//
// Reads benchmarks/minimax/artifacts/runs/<TASK>-<ts>/{evidence.json,
// telemetry/usage.jsonl} and rebuilds, via the shared renderer:
//   benchmarks/minimax/artifacts/reports/SUMMARY.md
//   benchmarks/minimax/artifacts/reports/kpi.json
//
// READ-ONLY w.r.t. runs/. Never launches a solver, never touches docker.
//
// Usage:
//   node benchmarks/minimax/summarize.mjs

import path from "node:path";
import { fileURLToPath } from "node:url";
import { loadMinimalAgentRows, renderStandardReport, kfmt, usd2 } from "../lib/standard-report.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const MODEL = { slug: "minimax-m3", id: "minimax/minimax-m3:free", label: "MiniMax-M3", provider: "OpenRouter" };

const RUNS_DIR = path.join(__dirname, "artifacts", "runs");
const REPORTS_DIR = path.join(__dirname, "artifacts", "reports");

const rows = loadMinimalAgentRows(RUNS_DIR);

const s = renderStandardReport({
  model: { slug: MODEL.slug, id: MODEL.id, label: MODEL.label },
  rows,
  reportsDir: REPORTS_DIR,
  generator: "benchmarks/minimax/summarize.mjs",
  suite: "XBOW-104",
  solver: "minimal-agent-in-container",
  header: {
    title: `# minimal-agent benchmark — ${MODEL.label} (\`${MODEL.id}\`, ${MODEL.provider})`,
    solverLine:
      "Solver: minimal-agent runtime in-container (xbow-agent-runner:latest). Suite: XBOW-104.",
  },
});

console.log(
  `summarize: ${MODEL.label} — ${s.attempted} attempted, ${s.solved} solved ` +
    `(${s.solveRate.toFixed(1)}%), ${kfmt(s.tot.in_plus_out)} tok (in+out), ` +
    `cost ${s.costKnown ? usd2(s.totalCost) : "-"}.`,
);
console.log(`  wrote ${path.join(REPORTS_DIR, "SUMMARY.md")}`);
console.log(`  wrote ${path.join(REPORTS_DIR, "kpi.json")}`);
