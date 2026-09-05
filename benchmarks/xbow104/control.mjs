// XBOW-104 shared utilities — path resolution, task locks, progress tracking.

import { randomUUID } from "node:crypto";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

/** Reject limits that overload the benchmark host or overflow Node timers. */
export function validateRunLimits(concurrency, timeoutS) {
  if (!Number.isInteger(concurrency) || concurrency < 1 || concurrency > 5)
    throw new Error("--concurrency must be an integer between 1 and 5");
  if (!Number.isFinite(timeoutS) || timeoutS <= 0 || timeoutS > 2147483)
    throw new Error("--timeout must be a positive finite number no greater than 2147483 seconds");
}

/** Remove only this Compose project's named tags; never force IDs or prune globally. */
export function cleanupTaskImages(project, { cwd, command = spawnSync } = {}) {
  const options = { cwd, encoding: "utf8", timeout: 60_000 };
  const listed = command("docker", [
    "images", "--filter", `reference=${project}-*`, "--format", "{{.Repository}}:{{.Tag}}",
  ], options);
  if (listed.status !== 0) return;
  const tags = [...new Set((listed.stdout || "").split(/\r?\n/)
    .map((tag) => tag.trim())
    .filter((tag) => tag.startsWith(`${project}-`) && tag.includes(":") && !tag.includes("<none>")))];
  if (tags.length) command("docker", ["rmi", ...tags], options);
}

/** Resolve the primary .env file (project root first, then benchmark dir). */
export function resolveBackboneFiles({ root, benchmark, env = process.env }) {
  const rootEnv = path.join(root, ".env");
  return {
    primary: path.resolve(
      env.XBOW104_ENV_FILE ||
        (fs.existsSync(rootEnv) ? rootEnv : path.join(benchmark, ".env")),
    ),
  };
}

/** Canonical artifact directory layout. Every path is overridable via env. */
export function resolveArtifactLayout({ benchmark, env = process.env }) {
  const artifactsDir = path.resolve(
    env.XBOW104_ARTIFACTS_DIR || path.join(benchmark, "artifacts"),
  );
  const runsDir = path.resolve(
    env.XBOW104_RUNS_DIR || path.join(artifactsDir, "runs"),
  );
  const reportsDir = path.resolve(
    env.XBOW104_REPORTS_DIR || path.join(artifactsDir, "reports"),
  );
  return {
    runsDir,
    reportsDir,
    summaryFile: path.resolve(
      env.XBOW104_SUMMARY_FILE || path.join(reportsDir, "SUMMARY.md"),
    ),
    resultsIndexFile: path.resolve(
      env.XBOW104_RESULTS_INDEX_FILE ||
        path.join(reportsDir, "results-index.json"),
    ),
    kpiMdFile: path.resolve(
      env.XBOW104_KPI_MD_FILE || path.join(reportsDir, "KPI.md"),
    ),
    kpiJsonFile: path.resolve(
      env.XBOW104_KPI_JSON_FILE || path.join(reportsDir, "kpi.json"),
    ),
  };
}

/** Check whether a process is still alive (cross-platform). */
export function processIsAlive(pid) {
  if (!Number.isInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (err) {
    return err?.code === "EPERM";
  }
}

/**
 * Exclusive file-based task lock.  Stale locks from dead PIDs are reclaimed.
 * Returns { file, release() }.
 */
export function acquireTaskLock(runsDir, task) {
  const dir = path.join(runsDir, ".locks");
  const file = path.join(dir, `${task}.lock`);
  const token = randomUUID();
  fs.mkdirSync(dir, { recursive: true });

  for (let attempt = 0; attempt < 2; attempt++) {
    try {
      const fd = fs.openSync(file, "wx");
      try {
        fs.writeFileSync(
          fd,
          JSON.stringify({
            task,
            pid: process.pid,
            token,
            started_at: new Date().toISOString(),
          }) + "\n",
        );
      } finally {
        fs.closeSync(fd);
      }
      return {
        file,
        release() {
          try {
            const cur = JSON.parse(fs.readFileSync(file, "utf8"));
            if (cur.token === token) fs.rmSync(file, { force: true });
          } catch (e) {
            if (e?.code !== "ENOENT") throw e;
          }
        },
      };
    } catch (e) {
      if (e?.code !== "EEXIST") throw e;
      let owner = null;
      try {
        owner = JSON.parse(fs.readFileSync(file, "utf8"));
      } catch {
        /* malformed → stale */
      }
      if (processIsAlive(Number(owner?.pid))) {
        throw new Error(
          `${task} already running (pid ${owner.pid}, started ${owner.started_at || "unknown"})`,
        );
      }
      fs.rmSync(file, { force: true });
    }
  }
  throw new Error(`could not acquire task lock for ${task}`);
}

/** Run an async operation with periodic progress logging. */
export async function runWithProgress(
  label,
  operation,
  { intervalMs = 15_000, log = console.log } = {},
) {
  const started = Date.now();
  const elapsed = () => `${Math.round((Date.now() - started) / 1000)}s`;
  log(`${label} started`);
  const timer = setInterval(
    () => log(`${label} waiting elapsed=${elapsed()}`),
    intervalMs,
  );
  timer.unref?.();
  try {
    const result = await operation();
    log(`${label} complete elapsed=${elapsed()}`);
    return result;
  } catch (err) {
    log(
      `${label} failed elapsed=${elapsed()}: ${String(err?.message || err).slice(0, 200)}`,
    );
    throw err;
  } finally {
    clearInterval(timer);
  }
}
