#!/usr/bin/env node
// Run ONE XBOW task by number or id, then refresh paper artifacts.
//
// Usage:
//   npm run xbow -- 42                   # XBEN-042-24
//   npm run xbow -- XBEN-042-24          # explicit id
//   npm run xbow -- 42 --dry-run         # print commands only

import { spawn, spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { writeCommandArtifacts } from "./artifacts.mjs";
import { resolveArtifactLayout } from "./control.mjs";
import { finalizeInterruptedRun, findLatestRun } from "./interrupt.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, "..", "..");
const { runsDir } = resolveArtifactLayout({ benchmark: __dirname });
const suiteRoot = path.resolve(
  process.env.XBOW104_SUITE_DIR ||
    path.join(__dirname, "..", "xbow-validation-benchmarks"),
);
let activeChild = null;
let interruptSignal = null;
let interruptCleanupComplete = false;

for (const sig of ["SIGINT", "SIGTERM"]) {
  process.on(sig, () => {
    if (interruptSignal) return;
    interruptSignal = sig;
    console.error(
      `\n[wrapper] ${sig} received; waiting for runner cleanup...`,
    );
    try {
      activeChild?.kill(sig);
    } catch {
      activeChild?.kill();
    }
    cleanupInterruptedTaskSync();
  });
}

const [first, ...rest] = process.argv.slice(2);
const wrapperFlags = new Set(["--dry-run", "--no-post"]);

const usage = () => {
  console.error("usage: npm run xbow -- <1-104|NN-NN|XBEN-id>");
  console.error(
    "  e.g. npm run xbow -- 42                    # runs XBEN-042-24",
  );
};

if (!first) {
  usage();
  process.exit(64);
}

let id = null;
if (/^XBEN-\d+-\d+$/.test(first)) id = first;
else {
  const m = first.match(/^(\d+)(?:-(\d+))?$/);
  if (m) id = `XBEN-${String(Number(m[1])).padStart(3, "0")}-${m[2] || "24"}`;
}
if (!id) {
  usage();
  process.exit(64);
}

const taskNumber = Number(id.match(/^XBEN-(\d+)-\d+$/)?.[1]);
if (!Number.isInteger(taskNumber) || taskNumber < 1 || taskNumber > 104) {
  console.error("task number must be between 1 and 104");
  process.exit(64);
}

const wantsDryRun = rest.includes("--dry-run");
const wantsNoPost = rest.includes("--no-post");
const runnerArgs = ["--tasks", id, "--rerun-all"];
const forwarded = rest.filter((flag) => !wrapperFlags.has(flag));
if (!forwarded.includes("--concurrency")) forwarded.push("--concurrency", "1");
runnerArgs.push(...forwarded);

const commands = [
  { label: "runner", file: "runner.mjs", args: runnerArgs },
  { label: "summary", file: "summary.mjs", args: [] },
  { label: "results-index", file: "build-results-index.mjs", args: [] },
  { label: "kpi", file: "kpi.mjs", args: [] },
];
if (wantsNoPost) commands.splice(1);

if (wantsDryRun) {
  for (const cmd of commands) {
    console.log(
      `would run [${cmd.label}]: node ${cmd.file} ${cmd.args.join(" ")}`.trim(),
    );
  }
  process.exit(0);
}

// Launch watchdog (detached)
const watchdog = spawn(
  process.execPath,
  [
    path.join(__dirname, "watchdog.mjs"),
    "--parent-pid",
    String(process.pid),
    "--task",
    id,
    "--runs-dir",
    runsDir,
    "--suite-root",
    suiteRoot,
    "--started-at",
    new Date().toISOString(),
  ],
  { detached: true, stdio: "ignore", windowsHide: true },
);
watchdog.unref();

function run(cmd) {
  return new Promise((resolve, reject) => {
    const child = spawn(
      process.execPath,
      [path.join(__dirname, cmd.file), ...cmd.args],
      { stdio: "inherit" },
    );
    activeChild = child;
    child.on("exit", (code) => {
      if (activeChild === child) activeChild = null;
      resolve(code ?? (interruptSignal ? 130 : 1));
    });
    child.on("error", reject);
  });
}

function cleanupInterruptedTaskSync() {
  if (interruptCleanupComplete) return;
  interruptCleanupComplete = true;
  const runDir = findLatestRun(runsDir, id);
  if (!runDir) return;
  const project = id.toLowerCase();
  const composeFile = path.join(
    suiteRoot,
    "benchmarks",
    id,
    "docker-compose.yml",
  );
  const cmd = (args) => {
    const r = spawnSync("docker", args, {
      cwd: projectRoot,
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
    });
    return {
      ok: r.status === 0,
      code: r.status ?? -1,
      stdout: r.stdout || "",
      stderr: r.stderr || r.error?.message || "",
    };
  };
  const agentCleanup = cmd(["rm", "-f", `${project}-agent`]);
  writeCommandArtifacts(runDir, "wrapper-interrupt-agent-cleanup", agentCleanup);
  const teardown = cmd([
    "compose",
    "-p",
    project,
    "-f",
    composeFile,
    "down",
    "-v",
  ]);
  writeCommandArtifacts(runDir, "wrapper-interrupt-compose-down", teardown);
  finalizeInterruptedRun(runsDir, id, interruptSignal || "signal", {
    teardownFailed: teardown.ok !== true,
  });
}

console.log(`[${id}] starting single-task pipeline...`);
for (const cmd of commands) {
  console.log(`[${id}] step=${cmd.label}`);
  const code = await run(cmd);
  if (interruptSignal) {
    cleanupInterruptedTaskSync();
    process.exit(130);
  }
  if (code !== 0) {
    console.error(
      `[${id}] ===== PIPELINE FAILED: ${cmd.label.toUpperCase()} =====`,
    );
    process.exit(code);
  }
}
console.log(`[${id}] ===== PIPELINE COMPLETE: REPORTS REFRESHED =====`);
