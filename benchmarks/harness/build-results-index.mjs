#!/usr/bin/env node
// Regenerate results-index.json from evidence.
//
// Emits only results-index.json so every model's reports/ dir stays the
// opus-clean trio (SUMMARY.md + kpi.json + results-index.json). The prior
// attempt-history.json projection was a duplicate of the per-attempt data
// already captured in kpi.json (totals.attempts / tasks) and is no longer
// written.

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
const { runsDir: OUT, resultsIndexFile: OUTPUT_FILE } =
  resolveArtifactLayout({ benchmark: __dirname });

const entries = loadAssessedEntries(OUT);
const newestValid = selectNewestValidEvidence(entries);
const newestAttempt = selectNewestEvidence(entries);

const tasks = [...newestValid.values()]
  .sort((a, b) => a.evidence.task.localeCompare(b.evidence.task))
  .map(({ stamp, evidence: ev }) => ({
    stamp,
    task: ev.task,
    level: String(ev.level),
    tags: ev.tags || [],
    solved: Boolean(ev.solved),
    timed_out: Boolean(ev.timed_out),
    exit_code: ev.exit_code,
    duration_s: ev.duration_s ?? 0,
    ...(ev.backbone ? { backbone: ev.backbone } : {}),
    model: ev.model,
    provider: ev.provider,
    outcome: ev.outcome,
    usage: {
      prompt_tokens: ev.usage?.prompt_tokens || 0,
      completion_tokens: ev.usage?.completion_tokens || 0,
      cached_tokens: ev.usage?.cached_tokens || 0,
      total_tokens: ev.usage?.total_tokens || 0,
      cost_usd: ev.usage?.cost_usd || 0,
    },
  }));

const excludedAttempts = [...newestAttempt.values()]
  .filter(({ evidence }) => evidence.valid_for_score !== true)
  .sort((a, b) => a.evidence.task.localeCompare(b.evidence.task))
  .map(({ evidence }) => ({
    task: evidence.task,
    outcome: evidence.outcome || "runtime_fault",
    exit_code: evidence.exit_code,
    provider: evidence.provider,
    model: evidence.model,
    provider_rate_limit_count: evidence.provider_rate_limit_count || 0,
  }));

const sum = (k) => tasks.reduce((a, t) => a + (t.usage[k] || 0), 0);
const solved = tasks.filter((t) => t.solved).length;
const index = {
  note: "newest valid attempt per task; excluded attempts listed separately; full per-attempt consumption in kpi.json",
  totals: {
    tasks_with_valid_evidence: tasks.length,
    solved,
    solve_rate: tasks.length
      ? Number(((solved / tasks.length) * 100).toFixed(1))
      : 0,
    prompt_tokens: sum("prompt_tokens"),
    completion_tokens: sum("completion_tokens"),
    total_tokens: sum("total_tokens"),
    cached_tokens: sum("cached_tokens"),
  },
  tasks,
  excluded_attempts: excludedAttempts,
};

fs.mkdirSync(path.dirname(OUTPUT_FILE), { recursive: true });
fs.writeFileSync(OUTPUT_FILE, JSON.stringify(index, null, 1) + "\n");
console.log(`results-index.json written: ${solved}/${tasks.length} solved`);
