#!/usr/bin/env node
// Standard report generator for the native (in-container) minimal-agent runtime
// benchmarks — GLM (z.ai) and DeepSeek (OpenRouter) and any future native model.
//
// Reads <artifacts>/runs/<TASK>-<ts>/{evidence.json, telemetry/usage.jsonl} and
// rebuilds, via the shared renderer, exactly the two opus-clean report files:
//   <artifacts>/reports/SUMMARY.md   (human report)
//   <artifacts>/reports/kpi.json     (machine KPIs)
// results-index.json is owned by build-results-index.mjs, not this generator.
//
// The artifacts directory resolves from XBOW104_ARTIFACTS_DIR when set (this is
// how benchmarks/harness/runner.mjs targets each model dir) or from a --model
// registry entry for manual runs. Model metadata comes from the registry when
// the model is known, otherwise it is derived read-only from evidence
// (provider/model). READ-ONLY w.r.t. runs/: never launches a solver, never
// touches docker, never mutates evidence. Safe to run while a live benchmark is
// in flight.
//
// Usage:
//   node benchmarks/zai/summarize.mjs --model glm-5.3-flash
//   node benchmarks/zai/summarize.mjs --model deepseek-v4-flash
//   XBOW104_ARTIFACTS_DIR=benchmarks/zai/glm-5.3-flash/artifacts \
//     node benchmarks/zai/summarize.mjs      # model derived from the dir/evidence

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { loadMinimalAgentRows, renderStandardReport, kfmt, usd2 } from "../harness/lib/standard-report.mjs";
import { resolveArtifactLayout } from "../harness/control.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const BENCH = path.resolve(__dirname, ".."); // benchmarks/

// Known native models. `dir` is the model's benchmark root (which holds artifacts/).
const MODELS = {
  "glm-5.3-flash": {
    id: "glm-5.3-flash",
    label: "GLM-5.3-Flash",
    provider: "z.ai",
    dir: path.join(BENCH, "zai", "glm-5.3-flash"),
  },
  "deepseek-v4-flash": {
    id: "deepseek-v4-flash",
    label: "DeepSeek-V4-Flash",
    provider: "openrouter",
    dir: path.join(BENCH, "deepseek-v4-flash"),
  },
};

const arg = (name, dflt) => {
  const i = process.argv.indexOf(`--${name}`);
  if (i < 0) return dflt;
  const v = process.argv[i + 1];
  return v && !v.startsWith("--") ? v : true;
};

// Recover provider/model from the first evidence.json when the model is unknown.
function deriveMeta(runsDir, slug) {
  try {
    for (const d of fs.readdirSync(runsDir).sort()) {
      if (!d.startsWith("XBEN-")) continue;
      const f = path.join(runsDir, d, "evidence.json");
      if (!fs.existsSync(f)) continue;
      const e = JSON.parse(fs.readFileSync(f, "utf8"));
      const id = e.model || slug;
      return { slug, id, label: id, provider: e.provider || "native" };
    }
  } catch {
    /* fall through to slug-only meta */
  }
  return { slug, id: slug, label: slug, provider: "native" };
}

const modelArg = arg("model", null);
const artifactsEnv = process.env.XBOW104_ARTIFACTS_DIR
  ? path.resolve(process.env.XBOW104_ARTIFACTS_DIR)
  : null;

let meta;
let benchmarkDir;
if (modelArg && MODELS[modelArg]) {
  meta = { slug: modelArg, ...MODELS[modelArg] };
  benchmarkDir = MODELS[modelArg].dir;
} else if (modelArg) {
  console.error(`--model must be one of: ${Object.keys(MODELS).join(", ")} (got "${modelArg}")`);
  process.exit(2);
} else if (artifactsEnv) {
  // Runner path: match the artifacts dir to a known model, else derive it.
  const matched = Object.entries(MODELS).find(
    ([, m]) =>
      artifactsEnv === path.join(m.dir, "artifacts") ||
      artifactsEnv.startsWith(m.dir + path.sep),
  );
  benchmarkDir = matched ? matched[1].dir : path.dirname(artifactsEnv);
  meta = matched
    ? { slug: matched[0], ...matched[1] }
    : deriveMeta(path.join(artifactsEnv, "runs"), path.basename(benchmarkDir));
} else {
  // Default harness path (no env, no --model): target benchmarks/harness/artifacts
  // and derive model metadata from the evidence there.
  benchmarkDir = path.join(BENCH, "harness");
  const defRuns = resolveArtifactLayout({ benchmark: benchmarkDir }).runsDir;
  meta = deriveMeta(defRuns, "native");
}

// benchmark=benchmarkDir; when XBOW104_ARTIFACTS_DIR is set it overrides unless --model was explicit.
const { runsDir: RUNS_DIR, reportsDir: REPORTS_DIR } = resolveArtifactLayout({
  benchmark: benchmarkDir,
  env: modelArg ? {} : process.env,
});

const rows = loadMinimalAgentRows(RUNS_DIR);

const s = renderStandardReport({
  model: { slug: meta.slug, id: meta.id, label: meta.label },
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
