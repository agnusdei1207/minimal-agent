import assert from "node:assert/strict";
import test from "node:test";
import fs from "node:fs";
import vm from "node:vm";
import * as targets from "../benchmarks/xbow104/target.mjs";
const { selectTargetService } = targets;

const exposed = { Service: "internal-service", Publishers: [{ TargetPort: 80, PublishedPort: 0, Protocol: "tcp" }] };
const app = { Service: "ssrf-demo-app", Publishers: [{ TargetPort: 80, PublishedPort: 49152, Protocol: "tcp" }] };

test("XBEN-020 picks the published SSRF entrypoint for either Compose service order", () => {
  for (const names of [["internal-service", "ssrf-demo-app"], ["ssrf-demo-app", "internal-service"]]) {
    assert.equal(selectTargetService(names, [exposed, app]), "ssrf-demo-app");
  }
});

test("published infrastructure never outranks the application", () => {
  const db = { Service: "db", Publishers: [{ TargetPort: 3306, PublishedPort: 49153 }] };
  assert.equal(selectTargetService(["db", "internal-service", "ssrf-demo-app"], [db, exposed, app]), "ssrf-demo-app");
  assert.equal(selectTargetService(["db", "web"], [db, { Service: "web", Publishers: [] }]), "web");
});

test("internal-only applications retain their existing fallback", () => {
  assert.equal(selectTargetService(["redis", "web"], [{ Service: "web", Publishers: null }]), "web");
  // Missing Compose state must no longer manufacture a reachable db:80.
  assert.throws(() => selectTargetService(["db"], []), /target discovery/);
});

test("UDP publication cannot outrank an HTTP application", () => {
  assert.equal(selectTargetService(["dns", "ssrf-demo-app"], [
    { Service: "dns", Publishers: [{ Protocol: "udp", TargetPort: 53, PublishedPort: 53000 }] }, app,
  ]), "ssrf-demo-app");
});

test("empty or missing selected Compose state fails discovery", () => {
  assert.throws(() => selectTargetService([], []), /target discovery/);
  assert.throws(() => selectTargetService(["missing", "web"], [{ Service: "web", Publishers: [] }]), /target discovery/);
});

test("only configured services with positive published ports are candidates", () => {
  assert.equal(selectTargetService(["internal-service", "ssrf-demo-app"], [
    null,
    { Service: "orphan", Publishers: [{ PublishedPort: 12345 }] },
    { Service: "internal-service", Publishers: [{ PublishedPort: null }, { PublishedPort: 0 }] },
    { Service: "ssrf-demo-app", Publishers: [{ PublishedPort: "49152" }] },
  ]), "ssrf-demo-app");
});

test("multiple published apps retain Compose order and missing rows do not imply publication", () => {
  assert.equal(selectTargetService(["missing", "second", "ssrf-demo-app"], [
    app, { Service: "second", Publishers: [{ PublishedPort: 49154 }] },
  ]), "second");
});

// Exercise each real discovery function without evaluating runner startup,
// which reads provider credentials and can launch paid benchmark work.
function loadDiscovery(source, name, context) {
  const declaration = source.match(new RegExp(`(?:export )?((?:async )?function ${name}\\([^]*?^\\})`, "m"));
  assert.ok(declaration, `runner discovery function ${name} must exist`);
  return vm.runInNewContext(`(${declaration[1]})`, context);
}

for (const runner of ["xbow104/runner.mjs", "claude/run.mjs", "minimax/run.mjs"]) {
  test(`${runner} reports discovery faults as start faults without hiding runtime failures`, () => {
    const source = fs.readFileSync(new URL(`../benchmarks/${runner}`, import.meta.url), "utf8");
    // Execute the production task failure handler in isolation from Docker and
    // credentials; its outcome determines whether the next solver may run.
    const handler = source.match(/} catch \(error\) \{([^]*?)^  }/m)[1];
    for (const [error, expected] of [
      [targets.targetDiscoveryError("fixture Compose failure"), "benchmark_start_fault"],
      [new Error("fixture runtime failure"), "runtime_fault"],
    ]) {
      const evidence = vm.runInNewContext(`${handler}\nev`, { error, ev: {}, started: Date.now() });
      assert.equal(evidence.outcome, expected);
      assert.equal(evidence.valid_for_score, false);
    }
  });

  test(`${runner} resolves XBEN-020 public host and container endpoints`, async () => {
    const source = fs.readFileSync(new URL(`../benchmarks/${runner}`, import.meta.url), "utf8");
    const context = {
      ...targets,
      selectTargetService,
      composeFilesArgs: (files) => files.flatMap((file) => ["-f", file]),
      sh: async () => ({ ok: true, stdout: "internal-service\nssrf-demo-app\n" }),
    };
    if (runner.startsWith("xbow104/")) {
      context.parseComposePsRows = loadDiscovery(source, "parseComposePsRows", context);
      context.resolveTargetDetails = loadDiscovery(source, "resolveTargetDetails", context);
    } else {
      context.parsePsRows = loadDiscovery(source, "parsePsRows", context);
    }
    for (const stdout of [JSON.stringify([exposed, app]), [exposed, app].map((row) => JSON.stringify(row)).join("\n")]) {
      context.compose = async () => ({ ok: true, stdout });
      const pickTarget = loadDiscovery(source, "pickTarget", context);
      const actual = await pickTarget("fixture", ["fixture-compose.yml"]);
      assert.equal(actual.service, "ssrf-demo-app");
      assert.equal(actual.hostUrl, "http://127.0.0.1:49152");
      assert.equal(actual.internalUrl, "http://ssrf-demo-app:80");
      assert.equal(actual.internalPort, 80);
    }
  });

  test(`${runner} honors TCP and internal exposed ports and rejects failed discovery`, async () => {
    const source = fs.readFileSync(new URL(`../benchmarks/${runner}`, import.meta.url), "utf8");
    const context = { ...targets, composeFilesArgs: (files) => files.flatMap((file) => ["-f", file]), sh: async () => ({ ok: true, stdout: "web\n" }) };
    if (runner.startsWith("xbow104/")) {
      context.parseComposePsRows = loadDiscovery(source, "parseComposePsRows", context);
      context.resolveTargetDetails = loadDiscovery(source, "resolveTargetDetails", context);
    } else context.parsePsRows = loadDiscovery(source, "parsePsRows", context);
    const pickTarget = loadDiscovery(source, "pickTarget", context);
    context.compose = async () => ({ ok: true, stdout: JSON.stringify([{ Service: "web", Publishers: [
      { Protocol: "udp", TargetPort: 53, PublishedPort: 53000 },
      { Protocol: "tcp", TargetPort: 8080, PublishedPort: 49152 },
    ] }]) });
    assert.equal((await pickTarget("fixture", ["compose.yml"])).internalUrl, "http://web:8080");
    context.compose = async () => ({ ok: true, stdout: JSON.stringify([{ Service: "web", Publishers: [{ Protocol: "tcp", TargetPort: 8080, PublishedPort: 0 }] }]) });
    const internal = await pickTarget("fixture", ["compose.yml"]);
    assert.equal(internal.internalUrl, "http://web:8080");
    assert.equal(internal.hostUrl, null);
    for (const response of [{ ok: false, stdout: "", stderr: "daemon failed" }, { ok: true, stdout: "[]" }, { ok: true, stdout: "{" }]) {
      context.compose = async () => response;
      await assert.rejects(pickTarget("fixture", ["compose.yml"]), /target discovery/);
    }
    context.sh = async () => ({ ok: false, stdout: "", stderr: "config failed" });
    await assert.rejects(pickTarget("fixture", ["compose.yml"]), /target discovery/);
  });
}
