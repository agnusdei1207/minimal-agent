import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
import test from "node:test";
import { cleanupTaskImages } from "../benchmarks/harness/control.mjs";

test("all native outcomes clean project image tags before writing finalized evidence", () => {
  const source = fs.readFileSync(new URL("../benchmarks/harness/runner.mjs", import.meta.url), "utf8");
  const finalizer = source.match(/const finalize = (\(\) => \{[^]*?^  });/m)[1];
  for (const outcome of ["benchmark_build_fault", "benchmark_start_fault", "interrupted", "solved"]) {
    for (const keep of [false, true]) {
      const order = [];
      const context = {
        ev: { outcome, duration_s: 1 }, id: "XBEN-020-24", proj: "xben-020-24",
        runDir: "/fixture/run", PROJECT_ROOT: "/fixture", startedAt: "fixture", started: Date.now(),
        process: { env: keep ? { XBOW104_KEEP_IMAGES: "1" } : {} },
        path: { join: (...parts) => parts.join("/") },
        cleanupTaskImages: (project, options) => cleanupTaskImages(project, { ...options, command: (_command, args) => {
          order.push(args[0]);
          return { status: 0, stdout: args[0] === "images" ? "xben-020-24-web:latest\n" : "" };
        } }),
        writeJsonAtomic: () => order.push("evidence"),
        writeState: () => order.push("state"),
        writeAuditManifest: () => order.push("manifest"),
      };
      vm.runInNewContext(`(${finalizer})()`, context);
      assert.deepEqual(order, keep ? ["evidence", "state", "manifest"] : ["images", "rmi", "evidence", "state", "manifest"], `${outcome}; keep=${keep}`);
    }
  }
});
