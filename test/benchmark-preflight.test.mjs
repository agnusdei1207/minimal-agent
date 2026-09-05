import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
import test from "node:test";
import * as control from "../benchmarks/harness/control.mjs";

for (const runner of ["harness/runner.mjs", "claude/run.mjs"]) {
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
  const source = fs.readFileSync(new URL(`../benchmarks/${runner}/run.mjs`, import.meta.url), "utf8");
  const declaration = source.match(/(async function main\([^]*?^\})/m);
  assert.ok(declaration);
  return vm.runInNewContext(`(${declaration[1]})`, {
    CONCURRENCY: 1, TIMEOUT_S: 900,
    validateRunLimits: control.validateRunLimits,
    process: { env: { OPENAI_API_KEY: "fixture", OPENAI_BASE_URL: "fixture" }, execPath: "fixture-node" },
    PATCH_SUITE: "fixture-patch.mjs",
    spawnSync: () => ({ status: 0, stdout: "" }),
    console: { log() {}, error() {} },
    RUNS_DIR: "fixture-runs",
    fs: { mkdirSync() { throw new Error("preflight unexpectedly reached run creation"); } },
    ...overrides,
  });
}

for (const runner of ["claude"]) {
  test(`${runner} rejects a failed or unlaunchable suite patch before creating runs`, async () => {
    for (const patch of [{ status: 1, stderr: "fixture suite failure" }, { status: null, error: new Error("fixture spawn failure") }]) {
      await assert.rejects(mainFor(runner, { spawnSync: () => patch })(), /suite patch failed/);
    }
  });
  test(`${runner} rejects invalid concurrency and timeout before running the suite patch`, async () => {
    for (const CONCURRENCY of [0, 6, 1.5, NaN, Infinity]) {
      await assert.rejects(mainFor(runner, {
        CONCURRENCY,
        spawnSync() { throw new Error("preflight unexpectedly patched suite"); },
      })(), /--concurrency/);
    }
    for (const TIMEOUT_S of [0, -1, NaN, Infinity]) {
      await assert.rejects(mainFor(runner, {
        TIMEOUT_S,
        spawnSync() { throw new Error("preflight unexpectedly patched suite"); },
      })(), /--timeout/);
    }
  });
}
