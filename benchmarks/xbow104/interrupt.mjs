// XBOW-104 interrupt handling — preserve evidence when Ctrl-C or parent death
// stops a run mid-flight.

import fs from "node:fs";
import path from "node:path";
import {
  writeAuditManifest,
  writeJsonAtomic,
  writeRunState,
} from "./artifacts.mjs";

/** Find the newest run directory for a given task id. */
export function findLatestRun(runsDir, task) {
  if (!fs.existsSync(runsDir)) return null;
  const name = fs
    .readdirSync(runsDir, { withFileTypes: true })
    .filter((e) => e.isDirectory() && e.name.startsWith(`${task}-`))
    .map((e) => e.name)
    .sort()
    .at(-1);
  return name ? path.join(runsDir, name) : null;
}

/** Cleanup may only act on this wrapper's latest attempt, never a successor. */
export function findOwnedLatestRun(runsDir, task, wrapperToken) {
  if (!wrapperToken) return null;
  const runDir = findLatestRun(runsDir, task);
  if (!runDir) return null;
  try {
    const state = JSON.parse(fs.readFileSync(path.join(runDir, "run-state.json"), "utf8"));
    return state.wrapper_token === wrapperToken ? runDir : null;
  } catch {
    return null;
  }
}

/** Finalize the latest run for a task that was interrupted before evidence.json. */
export function finalizeInterruptedRun(
  runsDir,
  task,
  signal,
  { teardownFailed = false } = {},
) {
  const runDir = findLatestRun(runsDir, task);
  if (!runDir) return { runDir: null, finalized: false };
  return finalizeInterruptedDirectory(runDir, task, signal, {
    teardownFailed,
  });
}

/** Finalize a specific run directory that was interrupted. */
export function finalizeInterruptedDirectory(
  runDir,
  task,
  signal,
  { teardownFailed = false } = {},
) {
  const evFile = path.join(runDir, "evidence.json");
  if (fs.existsSync(evFile)) return { runDir, finalized: false };

  let state = {};
  try {
    state = JSON.parse(
      fs.readFileSync(path.join(runDir, "run-state.json"), "utf8"),
    );
  } catch {
    /* preserve partial */
  }
  const started = Date.parse(state.started_at || "");
  const duration = Number.isFinite(started)
    ? Math.max(0, Math.round((Date.now() - started) / 1000))
    : 0;

  const evidence = {
    task,
    solved: false,
    outcome: "interrupted",
    valid_for_score: false,
    exit_signal: signal,
    error: `wrapper recovered an interrupted run after ${signal}`,
    teardown_failed: teardownFailed,
    duration_s: duration,
  };
  writeJsonAtomic(evFile, evidence);
  writeRunState(runDir, {
    task,
    phase: "finalized",
    started_at: state.started_at,
    outcome: evidence.outcome,
  });
  writeAuditManifest(runDir, { complete: true });
  return { runDir, finalized: true, evidence };
}
