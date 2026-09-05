#!/usr/bin/env node
// Standard report generator for the z.ai GLM minimal-agent benchmark.
//
// Reads benchmarks/zai/<model>/artifacts/runs/<TASK>-<ts>/{evidence.json,
// telemetry/usage.jsonl} and rebuilds, via the shared renderer:
//   benchmarks/zai/<model>/artifacts/reports/SUMMARY.md
//   benchmarks/zai/<model>/artifacts/reports/kpi.json
//
// READ-ONLY w.r.t. runs/. Never launches a solver, never touches docker.
//
// Usage:
//   node benchmarks/zai/summarize.mjs                       # defaults to glm-5.3-flash
//   node benchmarks/zai/summarize.mjs --model glm-5.3-flash

import path from "node:path";
import { fileURLToPath } from "node:url";
import { loadMinimalAgentRows, renderStandardReport, kfmt, usd2 } from "../harness/lib/standard-report.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const MODELS = {
  "glm-5.3-flash": { id: "glm-5.3-flash", label: "GLM-5.3-Flash", provider: "z.ai" },
};

const arg = (name, dflt) => {
  const i = process.argv.indexOf(`--${name}`);
  if (i < 0) return dflt;
  const v = process.argv[i + 1];
  return v && !v.startsWith("--") ? v : true;
};

const slug = String(arg("model", "glm-5.3-flash"));
const meta = MODELS[slug];
if (!meta) {
  console.error(`--model must be one of: ${Object.keys(MODELS).join(", ")} (got "${slug}")`);
  process.exit(2);
}

const MODEL_DIR = path.join(__dirname, slug);
const RUNS_DIR = path.join(MODEL_DIR, "artifacts", "runs");
const REPORTS_DIR = path.join(MODEL_DIR, "artifacts", "reports");

const rows = loadMinimalAgentRows(RUNS_DIR);

const s = renderStandardReport({
  model: { slug, id: meta.id, label: meta.label },
  rows,
  attempts: loadMinimalAgentRows(RUNS_DIR, { allAttempts: true }),
  reportsDir: REPORTS_DIR,
  generator: "benchmarks/zai/summarize.mjs",
  suite: "XBOW-104",
  solver: "minimal-agent-in-container",
  header: {
    title: `# minimal-agent benchmark — ${meta.label} (\`${meta.id}\`, ${meta.provider})`,
    solverLine:
      "Solver: minimal-agent runtime in-container (xbow-agent-runner:latest). Suite: XBOW-104.",
  },
});

console.log(
  `summarize: ${meta.label} — ${s.attempted} attempted, ${s.solved} solved ` +
    `(${s.solveRate.toFixed(1)}%), ${kfmt(s.tot.in_plus_out)} tok (in+out), ` +
    `cost ${s.costKnown ? usd2(s.totalCost) : "-"}.`,
);
console.log(`  wrote ${path.join(REPORTS_DIR, "SUMMARY.md")}`);
console.log(`  wrote ${path.join(REPORTS_DIR, "kpi.json")}`);
