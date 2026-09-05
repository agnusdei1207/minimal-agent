import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
import { EventEmitter } from "node:events";
import { setImmediate as nextTick } from "node:timers/promises";
import test from "node:test";
import * as control from "../benchmarks/harness/control.mjs";

const read = (file) => fs.readFileSync(new URL(`../benchmarks/${file}`, import.meta.url), "utf8");

for (const file of ["harness/runner.mjs"]) {
  test(`${file} waits for timeout container cleanup before resolving the attempt`, async () => {
    const source = read(file).split("const result = await runWithProgress(")[1];
    const expression = source.match(/new Promise\(\(resolve\) => \{[^]*?^        \}\)/m)[0];
    const child = new EventEmitter();
    child.kill = () => child.emit("exit", null, "SIGKILL");
    const timers = [];
    const pending = [];
    const commands = [];
    const command = async (...args) => {
      commands.push(args.flat().filter((item) => typeof item === "string"));
      return new Promise((resolve) => pending.push(resolve));
    };
    let finished = false;
    const promise = vm.runInNewContext(expression, {
      child, ev: {}, TIMEOUT_S: 1, agentName: "fixture-agent", AbortController,
      recordedCommand: command, sh: command,
      setTimeout: (callback) => { timers.push(callback); return 1; },
      clearTimeout() {},
    }).then((result) => { finished = true; return result; });
    timers[0]();
    await nextTick();
    assert.equal(finished, false, "evidence must not finalize while docker kill is outstanding");
    pending.shift()({ ok: true });
    await nextTick();
    assert.equal(finished, false, "evidence must not finalize while docker rm is outstanding");
    pending.shift()({ ok: true });
    const result = await promise;
    assert.equal(result.code, "timeout");
    assert.equal(commands.length, 2);
  });
}

for (const file of ["harness/runner.mjs", "claude/run.mjs"]) {
  test(`${file} removes only owned image tags without force or global prune`, async () => {
    const source = read(file);
    const calls = [];
    const spawnSync = (_cmd, args) => {
      calls.push(Array.from(args));
      return { status: 0, stdout: args[0] === "images" ? "fixture-web:latest\nfixture-web:latest\nfixture-other-web:latest\nfixtureevil:latest\n<none>:<none>\nsha256:abc\n" : "" };
    };
    const context = {
      ...control, spawnSync,
      cleanupTaskImages: (...args) => control.cleanupTaskImages(args[0], { ...args[1], command: spawnSync }),
      KEEP_IMAGES: false, process: { env: {} }, PROJECT_ROOT: "/fixture", proj: "fixture",
      compose: async () => ({ ok: true }),
    };
    if (file.startsWith("harness")) {
      const block = source.match(/if \(process\.env\.XBOW104_KEEP_IMAGES !== "1"\) \{[^]*?^    \}/m)[0];
      vm.runInNewContext(block, context);
    } else {
      const declaration = source.match(/async function teardown\([^]*?^\}/m)[0];
      await vm.runInNewContext(`(${declaration})`, context)("fixture", []);
    }
    assert.deepEqual(calls, [
      ["images", "--filter", "reference=fixture-*", "--format", "{{.Repository}}:{{.Tag}}"],
      ["rmi", "fixture-web:latest", "fixture-other-web:latest"],
    ]);
  });
}
