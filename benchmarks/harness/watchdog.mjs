#!/usr/bin/env node
// XBOW-104 watchdog — detached process that cleans up orphaned containers
// when the parent runner exits abnormally (crash, kill -9, terminal close).

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { writeCommandArtifacts } from "./artifacts.mjs";
import { acquireTaskLock, processIsAlive } from "./control.mjs";
import { finalizeInterruptedDirectory, findOwnedLatestRun } from "./interrupt.mjs";

const value = (name) => {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : null;
};
const parentPid = Number(value("parent-pid"));
const wrapperToken = value("wrapper-token");
const task = value("task");
const runsDir = path.resolve(value("runs-dir") || "");
const suiteRoot = path.resolve(value("suite-root") || "");
const startedAt = Date.parse(value("started-at") || "");

if (!Number.isInteger(parentPid) || parentPid <= 0 || !task || !wrapperToken || !runsDir || !suiteRoot)
  process.exit(64);

function command(runDir, label, args) {
  const result = spawnSync("docker", args, {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    windowsHide: true,
    timeout: 60_000,
  });
  const normalized = {
    ok: result.status === 0,
    code: result.status ?? -1,
    stdout: result.stdout || "",
    stderr: result.stderr || result.error?.message || "",
  };
  writeCommandArtifacts(runDir, label, normalized);
  return normalized;
}

function cleanupOrphan() {
  const runDir = findOwnedLatestRun(runsDir, task, wrapperToken);
  if (!runDir || fs.existsSync(path.join(runDir, "evidence.json"))) return;

  let state = {};
  try {
    state = JSON.parse(
      fs.readFileSync(path.join(runDir, "run-state.json"), "utf8"),
    );
  } catch {
    /* partial data preserved by finalizer */
  }
  // A surviving runner owns its cleanup even if its wrapper was killed.
  if (processIsAlive(Number(state.runner_pid))) return false;
  const runStarted = Date.parse(state.started_at || "");
  if (
    Number.isFinite(startedAt) &&
    Number.isFinite(runStarted) &&
    runStarted < startedAt - 5_000
  )
    return;

  let lock;
  try {
    lock = acquireTaskLock(runsDir, task);
  } catch {
    return;
  }
  try {
    if (findOwnedLatestRun(runsDir, task, wrapperToken) !== runDir) return;
    const project = task.toLowerCase();
    const composeFile = path.join(
      suiteRoot,
      "benchmarks",
      task,
      "docker-compose.yml",
    );
    command(runDir, "watchdog-agent-cleanup", ["rm", "-f", `${project}-agent`]);
    const teardown = command(runDir, "watchdog-compose-down", [
      "compose",
      "-p",
      project,
      "-f",
      composeFile,
      "down",
      "-v",
    ]);
    finalizeInterruptedDirectory(runDir, task, "parent_exit", {
      teardownFailed: !teardown.ok,
    });
  } finally {
    lock.release();
  }
}

const timer = setInterval(() => {
  if (processIsAlive(parentPid)) return;
  try {
    if (cleanupOrphan() === false) return;
  } catch {
    /* doctor will flag the incomplete-run */
  }
  clearInterval(timer);
  process.exit(0);
}, 500);
