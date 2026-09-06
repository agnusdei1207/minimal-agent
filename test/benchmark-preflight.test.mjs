import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
import test from "node:test";
import * as control from "../benchmarks/harness/control.mjs";

for (const runner of ["harness/runner.mjs"]) {
  test(`${runner} rejects bare numeric options before launching work`, () => {
    const source = fs.readFileSync(new URL(`../benchmarks/${runner}`, import.meta.url), "utf8");
    const parser = source.match(/const arg = [^]*?^};/m)[0];
    const constants = ["TIMEOUT_S", "CONCURRENCY"].map((name) => source.match(new RegExp(`^const ${name} = .+;`, "m"))[0]).join("\n");
    for (const option of ["--timeout", "--concurrency"]) {
      const limits = vm.runInNewContext(`${parser}\n${constants}\n({TIMEOUT_S, CONCURRENCY})`, { process: { argv: ["node", "runner.mjs", option, "--all"] } });
      assert.throws(() => control.validateRunLimits(limits.CONCURRENCY, limits.TIMEOUT_S), new RegExp(option));
    }
  });
}

function mainFor(runner, overrides = {}) {
  const source = fs.readFileSync(new URL(`../benchmarks/${runner}`, import.meta.url), "utf8");
  const declaration = source.match(/(async function main\([^]*?^\})/m);
  assert.ok(declaration);
  return vm.runInNewContext(`(${declaration[1]})`, {
    CONCURRENCY: 1, TIMEOUT_S: 900,
    validateRunLimits: control.validateRunLimits,
    arg: () => false,
    listTasks: () => [],
    doneTasks: () => new Map(),
    activeRuntimeModel: () => "fixture-model",
    BACKBONE: { name: "fixture-backbone" },
    HINTS: false,
    process: { env: { OPENAI_API_KEY: "fixture", OPENAI_BASE_URL: "fixture" }, execPath: "fixture-node", argv: ["node", "runner.mjs"], exitCode: 0 },
    spawnSync: () => ({ status: 0, stdout: "" }),
    console: { log() {}, error() {} },
    OUT: "fixture-runs",
    fs: { mkdirSync() { throw new Error("preflight unexpectedly reached run creation"); } },
    ...overrides,
  });
}

for (const runner of ["harness/runner.mjs"]) {
  test(`${runner} rejects invalid concurrency and timeout before creating runs`, async () => {
    for (const CONCURRENCY of [0, 13, 1.5, NaN, Infinity]) {
      await assert.rejects(mainFor(runner, {
        CONCURRENCY,
        fs: { mkdirSync() { throw new Error("preflight unexpectedly reached run creation"); } },
      })(), /--concurrency/);
    }
    for (const TIMEOUT_S of [0, -1, NaN, Infinity]) {
      await assert.rejects(mainFor(runner, {
        TIMEOUT_S,
        fs: { mkdirSync() { throw new Error("preflight unexpectedly reached run creation"); } },
      })(), /--timeout/);
    }
  });
}
