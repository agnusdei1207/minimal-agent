#!/usr/bin/env node
// Pre-flight diagnostics — verify backbone, suite, Docker, and agent image
// before a benchmark run.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { resolveArtifactLayout, resolveBackboneFiles } from "./control.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "..", "..");
const noDocker = process.argv.includes("--no-docker");
const { primary: envFile } = resolveBackboneFiles({
  root,
  benchmark: here,
});
const suiteRoot = path.resolve(
  process.env.XBOW104_SUITE_DIR ||
    path.join(here, "..", "xbow-validation-benchmarks"),
);
const { runsDir } = resolveArtifactLayout({ benchmark: here });
const image = process.env.XBOW104_AGENT_IMAGE || "xbow-agent-runner:latest";

function parseEnv(file) {
  if (!fs.existsSync(file)) return {};
  const vals = {};
  for (const line of fs.readFileSync(file, "utf8").split(/\r?\n/)) {
    const m = line.match(/^\s*([A-Z_][A-Z0-9_]*)\s*=\s*(.*)\s*$/);
    if (m && m[2]) vals[m[1]] = m[2].replace(/^["']|["']$/g, "");
  }
  return vals;
}

function backbone(vals) {
  const anth = [
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
  ];
  const oai = ["OPENAI_API_KEY", "OPENAI_BASE_URL", "OPENAI_MODEL"];
  try {
    if (anth.every((k) => vals[k]))
      return {
        complete: true,
        kind: "anthropic-compatible",
        model: vals.ANTHROPIC_DEFAULT_HAIKU_MODEL,
        host: new URL(vals.ANTHROPIC_BASE_URL).hostname,
      };
    if (oai.every((k) => vals[k]))
      return {
        complete: true,
        kind: "openai-compatible",
        model: vals.OPENAI_MODEL,
        host: new URL(vals.OPENAI_BASE_URL).hostname,
      };
  } catch {
    return { complete: false, kind: "invalid-url", model: "", host: "" };
  }
  return { complete: false, kind: "unconfigured", model: "", host: "" };
}

const checks = [];
const add = (name, status, detail) => checks.push({ name, status, detail });

// Backbone
const bb = backbone(parseEnv(envFile));
add(
  "primary_backbone",
  bb.complete ? "PASS" : "FAIL",
  bb.complete ? `${bb.kind}:${bb.host}/${bb.model}` : "not configured",
);

// Suite
const suite = path.join(suiteRoot, "benchmarks");
const taskCount = fs.existsSync(suite)
  ? fs
      .readdirSync(suite, { withFileTypes: true })
      .filter(
        (e) => e.isDirectory() && /^XBEN-\d+-\d+$/.test(e.name),
      ).length
  : 0;
add("suite_task_count", taskCount === 104 ? "PASS" : "FAIL", String(taskCount));

// Incomplete runs
const incomplete = fs.existsSync(runsDir)
  ? fs
      .readdirSync(runsDir, { withFileTypes: true })
      .filter((e) => {
        if (!e.isDirectory() || !e.name.startsWith("XBEN-")) return false;
        return (
          fs.existsSync(path.join(runsDir, e.name, "run-state.json")) &&
          !fs.existsSync(path.join(runsDir, e.name, "evidence.json"))
        );
      }).length
  : 0;
add("incomplete_runs", incomplete ? "WARN" : "PASS", String(incomplete));

// Docker
if (noDocker) {
  add("docker_checks", "SKIP", "--no-docker");
} else {
  const docker = spawnSync(
    "docker",
    ["version", "--format", "{{.Server.Version}}"],
    { encoding: "utf8", timeout: 30_000 },
  );
  add(
    "docker_daemon",
    docker.status === 0 ? "PASS" : "FAIL",
    docker.status === 0 ? docker.stdout.trim() : "unavailable",
  );
  const inspect = spawnSync(
    "docker",
    ["image", "inspect", image, "--format", "{{.Id}}"],
    { encoding: "utf8", timeout: 30_000 },
  );
  add(
    "agent_image",
    inspect.status === 0 ? "PASS" : "FAIL",
    inspect.status === 0 ? image : "missing",
  );
  if (inspect.status === 0) {
    // Attached endpoints are normal during a run. Empty networks are only
    // cleanup candidates: a concurrent compose up may still be creating them.
    try {
      const listing = spawnSync(
        "docker",
        [
          "network",
          "ls",
          "--filter",
          "name=xben-",
          "--format",
          "{{.Name}}",
        ],
        { encoding: "utf8", timeout: 30_000 },
      );
      if (listing.status !== 0) throw new Error("network listing failed");
      const nets = listing.stdout.trim()
        .split(/\r?\n/)
        .filter(Boolean);
      let networks = [];
      if (nets.length) {
        const inspectNetworks = spawnSync("docker", ["network", "inspect", ...nets], {
          encoding: "utf8", timeout: 30_000,
        });
        if (inspectNetworks.status !== 0) throw new Error("network inspection failed");
        networks = JSON.parse(inspectNetworks.stdout);
      }
      const active = networks.filter((net) => Object.keys(net.Containers || {}).length > 0).map((net) => net.Name);
      const empty = networks.filter((net) => Object.keys(net.Containers || {}).length === 0).map((net) => net.Name);
      add("active_xben_networks", "PASS", active.join(",") || "0");
      add("empty_xben_networks", empty.length ? "WARN" : "PASS", empty.join(",") || "0");
    } catch {
      add("xben_network_inspection", "WARN", "query failed; ownership unknown, do not clean up");
    }
  }
}

for (const c of checks)
  console.log(`${c.name} ${c.status} ${c.detail}`);
const failures = checks.filter((c) => c.status === "FAIL").length;
console.log(
  `doctor_result ${failures ? "FAIL" : "PASS"} failures=${failures} warnings=${checks.filter((c) => c.status === "WARN").length}`,
);
process.exitCode = failures ? 1 : 0;
