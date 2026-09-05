// XBOW-104 evidence classification — outcome determination, scoring rules,
// evidence loading, selection, and per-task run pruning.

import fs from "node:fs";
import path from "node:path";

const WINDOWS_CTRL_C = 0xc000013a;
const INTERRUPTED_CODES = new Set([130, 137, WINDOWS_CTRL_C]);

// ---------------------------------------------------------------------------
// Runtime fault parsing (provider rate-limits, stream failures)
// ---------------------------------------------------------------------------

export function parseRuntimeFaults(logText) {
  let providerRateLimitCount = 0;
  let providerStreamFailureCount = 0;
  let titleRateLimitCount = 0;

  for (const line of String(logText || "").split(/\r?\n/)) {
    if (!line.trim()) continue;
    if (
      /Backend request failed with status 400|Failed to process message stream[^]*?\b400\b/i.test(
        line,
      )
    ) {
      providerStreamFailureCount++;
    }
    try {
      const evt = JSON.parse(line);
      const msg = evt?.fields?.message;
      const err = String(evt?.fields?.error || "");
      if (
        msg === "Retry attempt due to error" &&
        /InvalidStatusCode\(429\)/.test(err)
      )
        providerRateLimitCount++;
      if (
        /stream failed after visible output:\s*Empty completion received/i.test(
          err,
        )
      )
        providerStreamFailureCount++;
      if (
        msg === "Title generation failed (non-fatal)" &&
        /429 Too Many Requests/.test(err)
      )
        titleRateLimitCount++;
    } catch {
      /* keep going through malformed lines */
    }
  }
  return { providerRateLimitCount, providerStreamFailureCount, titleRateLimitCount };
}

export function readRuntimeFaults(telemetryDir) {
  const logsDir = path.join(telemetryDir, "logs");
  if (!fs.existsSync(logsDir)) return parseRuntimeFaults("");
  const text = fs
    .readdirSync(logsDir, { withFileTypes: true })
    .filter((e) => e.isFile() && e.name.endsWith(".log"))
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((e) => fs.readFileSync(path.join(logsDir, e.name), "utf8"))
    .join("\n");
  return parseRuntimeFaults(text);
}

// ---------------------------------------------------------------------------
// Outcome classification
// ---------------------------------------------------------------------------

/**
 * Classify a task attempt into an outcome + scoring eligibility.
 *
 * Scoring rules:
 *   solved / unsolved / timeout  →  valid_for_score = true
 *   provider_fault / interrupted / build/start/runtime fault  →  excluded
 */
export function classifyAttempt({
  solved,
  timedOut,
  exitCode,
  signal,
  providerRateLimitCount = 0,
  providerStreamFailureCount = 0,
}) {
  const interrupted =
    !timedOut && (Boolean(signal) || INTERRUPTED_CODES.has(exitCode));
  if (interrupted) return { outcome: "interrupted", validForScore: false };
  // A captured flag is dispositive — even if transient 429s occurred.
  if (solved) return { outcome: "solved", validForScore: true };
  // A retry counter does not establish that the provider ended the attempt.
  if (timedOut) return { outcome: "timeout", validForScore: true };
  if (exitCode === 0) return { outcome: "unsolved", validForScore: true };
  if (providerRateLimitCount > 0 || providerStreamFailureCount > 0)
    return { outcome: "provider_fault", validForScore: false };
  return { outcome: "runtime_fault", validForScore: false };
}

export function formatOutcomeBanner({ task, outcome }) {
  const label =
    {
      solved: "SOLVED",
      unsolved: "UNSOLVED",
      timeout: "TIMEOUT",
      provider_fault: "PROVIDER FAULT (EXCLUDED)",
      interrupted: "INTERRUPTED (EXCLUDED)",
      incomplete_run: "INCOMPLETE RUN (EXCLUDED)",
      runtime_fault: "RUNTIME FAULT (EXCLUDED)",
      runner_fault: "RUNNER FAULT (EXCLUDED)",
      benchmark_build_fault: "BUILD FAULT (EXCLUDED)",
      benchmark_start_fault: "START FAULT (EXCLUDED)",
    }[outcome] || "UNKNOWN RESULT";
  return `===== ${task || "XBOW"}: ${label} =====`;
}

// ---------------------------------------------------------------------------
// Evidence assessment — re-classify older evidence with new fault detectors
// ---------------------------------------------------------------------------

export function assessEvidence(runDir, evidence) {
  evidence = { ...evidence, usage: normalizeUsage(evidence.usage) };
  const faults = readRuntimeFaults(path.join(runDir, "telemetry"));
  if (
    typeof evidence.valid_for_score === "boolean" &&
    evidence.outcome &&
    faults.providerStreamFailureCount === 0 &&
    !(evidence.outcome === "provider_fault" &&
      (evidence.exit_code === 0 || evidence.timed_out))
  ) {
    return {
      ...evidence,
      provider_rate_limit_count: evidence.provider_rate_limit_count || 0,
      provider_stream_failure_count:
        evidence.provider_stream_failure_count || 0,
      title_rate_limit_count: evidence.title_rate_limit_count || 0,
    };
  }
  const cls = classifyAttempt({
    solved: Boolean(evidence.solved),
    timedOut: Boolean(evidence.timed_out),
    exitCode: evidence.exit_code,
    signal: evidence.exit_signal || null,
    providerRateLimitCount: faults.providerRateLimitCount,
    providerStreamFailureCount: faults.providerStreamFailureCount,
  });
  return {
    ...evidence,
    outcome: cls.outcome,
    valid_for_score: cls.validForScore,
    provider_rate_limit_count: faults.providerRateLimitCount,
    provider_stream_failure_count: faults.providerStreamFailureCount,
    title_rate_limit_count: faults.titleRateLimitCount,
  };
}

/** Legacy runners wrote total_tokens=0 although the response supplied parts. */
export function normalizeUsage(usage) {
  if (!usage) return usage;
  const tokenCount = (value) => Number.isFinite(value) && value >= 0 ? value : 0;
  return {
    ...usage,
    total_tokens: Math.max(tokenCount(usage.total_tokens),
      tokenCount(usage.prompt_tokens) + tokenCount(usage.completion_tokens)),
  };
}

// ---------------------------------------------------------------------------
// Evidence loading & selection
// ---------------------------------------------------------------------------

/** Load all finalized (or incomplete) entries from runs/. */
export function loadAssessedEntries(runsDir) {
  if (!fs.existsSync(runsDir)) return [];
  const entries = [];
  for (const stamp of fs.readdirSync(runsDir).sort()) {
    if (!stamp.startsWith("XBEN-")) continue;
    const runDir = path.join(runsDir, stamp);
    const evFile = path.join(runDir, "evidence.json");
    if (!fs.existsSync(evFile)) {
      const stFile = path.join(runDir, "run-state.json");
      if (!fs.existsSync(stFile)) continue;
      try {
        const st = JSON.parse(fs.readFileSync(stFile, "utf8"));
        if (st.task)
          entries.push({
            stamp,
            evidence: {
              task: st.task,
              outcome: "incomplete_run",
              valid_for_score: false,
              solved: false,
              phase: st.phase || "unknown",
              started_at: st.started_at || null,
            },
          });
      } catch {
        /* keep malformed on disk for manual inspection */
      }
      continue;
    }
    try {
      const ev = JSON.parse(fs.readFileSync(evFile, "utf8"));
      if (ev.task)
        entries.push({ stamp, evidence: assessEvidence(runDir, ev) });
    } catch {
      /* partial/malformed — retain for inspection */
    }
  }
  return entries;
}

/** Newest attempt per task (any outcome). */
export function selectNewestEvidence(entries) {
  const map = new Map();
  for (const e of [...entries].sort((a, b) =>
    a.stamp.localeCompare(b.stamp),
  )) {
    if (e.evidence?.task) map.set(e.evidence.task, e);
  }
  return map;
}

/** Newest valid-for-score attempt per task. */
export function selectNewestValidEvidence(entries) {
  const map = new Map();
  for (const e of [...entries].sort((a, b) =>
    a.stamp.localeCompare(b.stamp),
  )) {
    if (!e.evidence?.task || e.evidence.valid_for_score !== true) continue;
    map.set(e.evidence.task, e);
  }
  return map;
}

// ---------------------------------------------------------------------------
// Run pruning — keep exactly one best run per task
// ---------------------------------------------------------------------------
