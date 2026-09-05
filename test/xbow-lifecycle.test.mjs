import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import vm from "node:vm";
import test from "node:test";
import { EventEmitter } from "node:events";
import * as control from "../benchmarks/xbow104/control.mjs";
import * as interrupt from "../benchmarks/xbow104/interrupt.mjs";

function loadFunction(file, name, context) {
  const source = fs.readFileSync(new URL(`../benchmarks/xbow104/${file}`, import.meta.url), "utf8");
  const match = source.match(new RegExp(`(?:export )?((?:async )?function ${name}\\([^]*?^\\})`, "m"));
  assert.ok(match, `${name} exists`);
  return vm.runInNewContext(`(${match[1]})`, context);
}

function mainFixture(options = {}) {
  const attempted = [];
  const context = {
    ...control, CONCURRENCY: 1, TIMEOUT_S: 60, NO_COMMIT: true, HINTS: true,
    OUT: "/fixture", BACKBONE: { name: "fixture" },
    arg: () => false, activeRuntimeModel: () => "fixture",
    fs: { mkdirSync() {}, writeFileSync() {} }, path,
    listTasks: () => ["XBEN-001-24", "XBEN-002-24"],
    doneTasks: () => new Map(), console: { log() {} },
    acquireTaskLock: () => ({ release() {} }),
    runCancellation: { signal: null }, process: {}, stamp: () => "fixture",
    ...options,
  };
  context.runTask = async (id) => {
    attempted.push(id);
    context.runCancellation.signal = "SIGTERM";
    return { task: id, solved: false, duration_s: 0 };
  };
  return { main: loadFunction("runner.mjs", "main", context), attempted };
}

test("cancellation stops assigning queued benchmark tasks", async () => {
  const { main, attempted } = mainFixture();
  await main();
  assert.deepEqual(attempted, ["XBEN-001-24"]);
});

for (const options of [{ CONCURRENCY: 6 }, { TIMEOUT_S: 0 }, { TIMEOUT_S: NaN }, { TIMEOUT_S: Infinity }]) {
  test(`invalid runner limits fail before starting tasks: ${JSON.stringify(options)}`, async () => {
    const { main, attempted } = mainFixture(options);
    await assert.rejects(main());
    assert.deepEqual(attempted, []);
  });
}

test("an old watchdog cannot tear down a newer attempt of the same task", (t) => {
  const runsDir = fs.mkdtempSync(path.join(os.tmpdir(), "xbow-watchdog-"));
  t.after(() => fs.rmSync(runsDir, { recursive: true, force: true }));
  const task = "XBEN-020-24";
  const runDir = path.join(runsDir, `${task}-2026-09-05T02-00-00-000Z`);
  fs.mkdirSync(runDir);
  fs.writeFileSync(path.join(runDir, "run-state.json"), JSON.stringify({
    task, runner_pid: 222, wrapper_token: "new-owner", started_at: "2026-09-05T02:00:00Z",
  }));
  const commands = [];
  const cleanup = loadFunction("watchdog.mjs", "cleanupOrphan", {
    ...interrupt, fs, path, runsDir, task, suiteRoot: "/fixture",
    parentPid: 111, runnerPid: 123, wrapperToken: "old-owner", startedAt: Date.parse("2026-09-05T01:00:00Z"),
    processIsAlive: () => false,
    command: (_dir, label) => { commands.push(label); return { ok: true }; },
  });
  cleanup();
  assert.deepEqual(commands, []);
  assert.equal(fs.existsSync(path.join(runDir, "evidence.json")), false);
});

test("doctor distinguishes connected networks from empty cleanup candidates", () => {
  const source = fs.readFileSync(new URL("../benchmarks/xbow104/doctor.mjs", import.meta.url), "utf8")
    .replace(/^import[^;]+;\s*/gm, "");
  const logs = [];
  const context = {
    ...control, path, fileURLToPath: () => "/fixture/benchmarks/xbow104/doctor.mjs",
    fs: { existsSync: () => false },
    process: { argv: [], env: {}, exitCode: 0 },
    console: { log: (line) => logs.push(line) },
    spawnSync: (_cmd, args) => {
      if (args[0] === "network" && args[1] === "ls") return { status: 0, stdout: "xben-live_default\nxben-empty_default\n" };
      if (args[0] === "network" && args[1] === "inspect") return { status: 0, stdout: JSON.stringify([
        { Name: "xben-live_default", Containers: { abc: { Name: "xben-live-web-1" } } },
        { Name: "xben-empty_default", Containers: {} },
      ]) };
      return { status: 0, stdout: "fixture\n" };
    },
  };
  vm.runInNewContext(source.replace(/import\.meta\.url/g, '"fixture"'), context);
  assert.ok(logs.includes("active_xben_networks PASS xben-live_default"));
  assert.ok(logs.includes("empty_xben_networks WARN xben-empty_default"));
  assert.equal(logs.some((line) => line.startsWith("orphan_xben_networks")), false);
});

test("watchdog recovers its dead runner but waits for its surviving runner", (t) => {
  const runsDir = fs.mkdtempSync(path.join(os.tmpdir(), "xbow-owned-watchdog-"));
  t.after(() => fs.rmSync(runsDir, { recursive: true, force: true }));
  const task = "XBEN-020-24";
  const runDir = path.join(runsDir, `${task}-2026-09-05T02-00-00-000Z`);
  fs.mkdirSync(runDir);
  fs.writeFileSync(path.join(runDir, "run-state.json"), JSON.stringify({
    task, runner_pid: 222, wrapper_token: "owner", started_at: "2026-09-05T02:00:00Z",
  }));
  let alive = true;
  const commands = [];
  const cleanup = loadFunction("watchdog.mjs", "cleanupOrphan", {
    ...control, ...interrupt, fs, path, runsDir, task, suiteRoot: "/fixture",
    wrapperToken: "owner", startedAt: Date.parse("2026-09-05T01:00:00Z"),
    processIsAlive: () => alive,
    command: (_dir, label) => { commands.push(label); return { ok: true }; },
  });
  assert.equal(cleanup(), false);
  assert.deepEqual(commands, []);
  alive = false;
  const lock = control.acquireTaskLock(runsDir, task);
  try {
    cleanup();
    assert.deepEqual(commands, [], "cleanup cannot race another task lock owner");
  } finally {
    lock.release();
  }
  cleanup();
  assert.deepEqual(commands, ["watchdog-agent-cleanup", "watchdog-compose-down"]);
  assert.equal(JSON.parse(fs.readFileSync(path.join(runDir, "evidence.json"), "utf8")).outcome, "interrupted");
  cleanup();
  assert.equal(commands.length, 2, "completed evidence prevents repeated cleanup");
});

test("runner setup builds through the owned image pipeline and preserves no-cache", async () => {
  const calls = [];
  const setup = loadFunction("runner.mjs", "setup", {
    process: { platform: "win32", env: { XBOW104_NO_CACHE: "1" } },
    path, PROJECT_ROOT: "/fixture", __dirname: "/fixture/benchmarks/xbow104",
    AGENT_IMAGE: "xbow-agent-runner:fixture", console: { log() {} },
    spawn: (command, args) => {
      calls.push({ command, args: Array.from(args) });
      const child = new EventEmitter();
      queueMicrotask(() => child.emit("exit", 0));
      return child;
    },
  });
  await setup();
  assert.deepEqual(calls, [{ command: "powershell", args: [
    "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "/fixture/scripts/dimage.ps1",
    "-Target", "runner", "-Tag", "xbow-agent-runner:fixture", "-NoCache",
  ] }]);
});
