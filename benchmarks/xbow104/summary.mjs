#!/usr/bin/env node
// Aggregate artifacts/runs/*/evidence.json into a paper-ready SUMMARY.md.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  loadAssessedEntries,
  selectNewestEvidence,
  selectNewestValidEvidence,
} from "./evidence.mjs";
import { resolveArtifactLayout } from "./control.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const {
  runsDir: OUT,
  summaryFile: SUMMARY_FILE,
  kpiMdFile: KPI_MD_FILE,
} = resolveArtifactLayout({ benchmark: __dirname });

const entries = loadAssessedEntries(OUT);
const validByTask = selectNewestValidEvidence(entries);
const latestByTask = selectNewestEvidence(entries);

const rows = [...validByTask.values()]
  .map((e) => e.evidence)
  .sort((a, b) => String(a.task).localeCompare(b.task));
const excluded = [...latestByTask.values()]
  .filter((e) => e.evidence.valid_for_score !== true)
  .map((e) => e.evidence)
  .sort((a, b) => String(a.task).localeCompare(b.task));

const solved = rows.filter((r) => r.solved);
const totalDur = rows.reduce((s, r) => s + (r.duration_s || 0), 0);
const tokens = rows.reduce(
  (s, r) => ({
    prompt: s.prompt + (r.usage?.prompt_tokens || 0),
    completion: s.completion + (r.usage?.completion_tokens || 0),
    cached: s.cached + (r.usage?.cached_tokens || 0),
    total: s.total + (r.usage?.total_tokens || 0),
  }),
  { prompt: 0, completion: 0, cached: 0, total: 0 },
);
const costMeasured = rows.some((r) => (r.usage?.cost_usd || 0) > 0);
const totalCost = costMeasured
  ? rows.reduce((s, r) => s + (r.usage?.cost_usd || 0), 0)
  : 0;

const levels = {};
for (const r of rows) {
  (levels[r.level] ??= { n: 0, s: 0 });
  levels[r.level].n += 1;
  if (r.solved) levels[r.level].s += 1;
}

const fmtM = (v) =>
  v >= 1e6 ? `${(v / 1e6).toFixed(2)}M` : `${Math.round(v / 1e3)}k`;
const model =
  [
    ...new Set(
      rows.map(
        (r) => `${r.provider || "-"}${r.model ? `/${r.model}` : ""}`,
      ),
    ),
  ].join(", ") || "(unset)";

const lines = [
  "# XBOW-104 Run Summary",
  "",
  `- Model/provider: **${model}**`,
  `- Solved: **${solved.length}/${rows.length}** (${rows.length ? ((100 * solved.length) / rows.length).toFixed(1) : 0}%)`,
  `- Excluded infrastructure attempts: **${excluded.length}**`,
  `- Sum of per-task durations: **${(totalDur / 3600).toFixed(2)} h**`,
  `- KPI companion: **${path.basename(KPI_MD_FILE)}**`,
  `- Tokens (from runtime telemetry): prompt **${fmtM(tokens.prompt)}**, completion **${fmtM(tokens.completion)}**, cached **${fmtM(tokens.cached)}**, total **${fmtM(tokens.total)}**`,
  costMeasured
    ? `- Measured cost (provider-reported): **$${totalCost.toFixed(4)}**`
    : "- Cost: not provider-reported",
  "",
  "| Level | Solved | Tasks | Rate |",
  "| --- | --- | --- | --- |",
  ...Object.entries(levels)
    .sort()
    .map(
      ([lv, v]) =>
        `| ${lv} | ${v.s} | ${v.n} | ${((100 * v.s) / v.n).toFixed(1)}% |`,
    ),
  "",
  "| Task | Level | Result | Duration(s) | Prompt tok | Compl tok | Total tok | Cost($) | Flags seen |",
  "| --- | --- | --- | --- | --- | --- | --- | --- | --- |",
  ...rows.map((r) => {
    const u = r.usage || {};
    const cost = u.cost_usd ? u.cost_usd.toFixed(4) : "";
    const result = r.solved ? "SOLVED" : r.timed_out ? "TIMEOUT" : "FAILED";
    return `| ${r.task} | ${r.level ?? ""} | ${result}${r.error ? `(${r.error})` : ""} | ${r.duration_s ?? ""} | ${u.prompt_tokens ?? ""} | ${u.completion_tokens ?? ""} | ${u.total_tokens ?? ""} | ${cost} | ${(r.flags_seen || []).length} |`;
  }),
  "",
  "## Excluded Infrastructure Attempts",
  "",
  "| Task | Outcome | Provider | Model | Rate-limit retries |",
  "| --- | --- | --- | --- | --- |",
  ...excluded.map(
    (r) =>
      `| ${r.task} | ${r.outcome || "runtime_fault"} | ${r.provider || ""} | ${r.model || ""} | ${r.provider_rate_limit_count || 0} |`,
  ),
  "",
];

fs.mkdirSync(path.dirname(SUMMARY_FILE), { recursive: true });
fs.writeFileSync(SUMMARY_FILE, lines.join("\n"));
console.log(`SUMMARY.md written: ${solved.length}/${rows.length} solved`);
