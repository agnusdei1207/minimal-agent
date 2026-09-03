#!/usr/bin/env node
// KPI extraction and paper-ready KPI.md + kpi.json generation.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  loadAssessedEntries,
  selectNewestEvidence,
  selectNewestValidEvidence,
} from "./evidence.mjs";
import { resolveArtifactLayout } from "./control.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const {
  runsDir: RUNS_DIR,
  kpiJsonFile: KPI_JSON_FILE,
  kpiMdFile: KPI_MD_FILE,
} = resolveArtifactLayout({ benchmark: __dirname });

function readText(file) {
  return fs.existsSync(file) ? fs.readFileSync(file, "utf8") : "";
}

function readJsonLines(file) {
  if (!fs.existsSync(file)) return [];
  const rows = [];
  for (const line of fs.readFileSync(file, "utf8").split(/\r?\n/)) {
    if (!line.trim()) continue;
    try {
      rows.push(JSON.parse(line));
    } catch {
      /* skip malformed */
    }
  }
  return rows;
}

function countMatches(text, pattern) {
  return [...String(text || "").matchAll(pattern)].length;
}

function percentile(values, frac) {
  if (!values.length) return 0;
  const sorted = [...values].map(Number).sort((a, b) => a - b);
  return sorted[Math.ceil(frac * sorted.length) - 1];
}

// ---------------------------------------------------------------------------
// Per-run KPI extraction
// ---------------------------------------------------------------------------

function summarizeTranscript(runDir) {
  const file = path.join(runDir, "transcript.txt");
  const text = readText(file);
  return {
    bytes: fs.existsSync(file) ? fs.statSync(file).size : 0,
    lines: text ? text.split(/\r?\n/).filter(Boolean).length : 0,
    failed_message_stream_count: countMatches(
      text,
      /Failed to process message stream/g,
    ),
  };
}

function summarizeRuntimeLogs(runDir) {
  const logsDir = path.join(runDir, "telemetry", "logs");
  const rows = fs.existsSync(logsDir)
    ? fs
        .readdirSync(logsDir, { withFileTypes: true })
        .filter((e) => e.isFile())
        .sort((a, b) => a.name.localeCompare(b.name))
        .flatMap((e) => readJsonLines(path.join(logsDir, e.name)))
    : [];
  const providerWaits = rows
    .filter((r) => r.event === "stage_end" && r.stage === "provider_wait")
    .map((r) => Number(r.duration_ms || 0));
  const toolEnds = rows.filter((r) => r.event === "tool_end");
  const responses = rows.filter((r) => r.event === "response");
  return {
    log_rows: rows.length,
    request_count: rows.filter((r) => r.event === "request_start").length,
    response_count: responses.length,
    tool_call_count: toolEnds.length,
    tool_failure_count: toolEnds.filter((r) => r.ok === false).length,
    shell_tool_calls: toolEnds.filter((r) => r.tool === "shell").length,
    shell_tool_failures: toolEnds.filter(
      (r) => r.tool === "shell" && r.ok === false,
    ).length,
    fetch_tool_failures: toolEnds.filter(
      (r) => r.tool === "fetch" && r.ok === false,
    ).length,
    slow_stage_count: rows.filter((r) => r.event === "slow_stage").length,
    provider_wait_total_ms: providerWaits.reduce((s, v) => s + v, 0),
    provider_wait_max_ms: providerWaits.length
      ? Math.max(...providerWaits)
      : 0,
    response_prompt_tokens: responses.reduce(
      (s, r) => s + Number(r.prompt_tokens || 0),
      0,
    ),
    response_total_tokens: responses.reduce(
      (s, r) => s + Number(r.total_tokens || 0),
      0,
    ),
    unique_tools: [
      ...new Set(toolEnds.map((r) => r.tool).filter(Boolean)),
    ].sort(),
  };
}

function summarizeShellListener(runDir) {
  const base = path.join(runDir, "telemetry", "shell-listener");
  const auditRows = readJsonLines(path.join(base, "audit.jsonl"));
  const eventRows = readJsonLines(path.join(base, "events.jsonl"));
  const snapshot = fs.existsSync(path.join(base, "sessions.snapshot.json"))
    ? JSON.parse(
        fs.readFileSync(path.join(base, "sessions.snapshot.json"), "utf8"),
      )
    : { sessions: [] };
  const sessions = Array.isArray(snapshot.sessions)
    ? snapshot.sessions
    : [];
  return {
    audit_request_count: auditRows.length,
    spawn_request_count: auditRows.filter(
      (r) => r.request?.op === "spawn",
    ).length,
    session_count: sessions.length,
    nonzero_exit_session_count: sessions.filter(
      (r) => Number(r.exit_status?.code || 0) !== 0,
    ).length,
  };
}

export function summarizeRunKpi(runDir, evidence) {
  const transcript = summarizeTranscript(runDir);
  const runtime = summarizeRuntimeLogs(runDir);
  const shellListener = summarizeShellListener(runDir);
  const issues = [];
  if (transcript.failed_message_stream_count > 0)
    issues.push("message_stream_failure");
  if (runtime.tool_failure_count > 0) issues.push("runtime_tool_failure");
  if (runtime.fetch_tool_failures > 0) issues.push("fetch_failure");
  if (shellListener.nonzero_exit_session_count > 0)
    issues.push("nonzero_shell_exit");
  if (evidence.outcome === "provider_fault") issues.push("provider_fault");
  if (evidence.outcome === "interrupted") issues.push("interrupted_run");
  if (evidence.outcome === "runtime_fault") issues.push("runtime_fault");
  if (evidence.outcome === "benchmark_build_fault")
    issues.push("benchmark_build_fault");
  if (evidence.outcome === "benchmark_start_fault")
    issues.push("benchmark_start_fault");
  if (evidence.outcome === "incomplete_run") issues.push("incomplete_run");
  if (evidence.teardown_failed === true) issues.push("teardown_failure");
  return {
    task: evidence.task,
    stamp: path.basename(runDir),
    benchmark: {
      level: evidence.level ?? null,
      outcome: evidence.outcome ?? null,
      valid_for_score: evidence.valid_for_score === true,
      solved: Boolean(evidence.solved),
      timed_out: Boolean(evidence.timed_out),
      duration_s: Number(evidence.duration_s || 0),
      provider: evidence.provider || "",
      model: evidence.model || "",
      backbone: evidence.backbone || "",
      teardown_failed: evidence.teardown_failed === true,
      provider_rate_limit_count: Number(
        evidence.provider_rate_limit_count || 0,
      ),
      flags_seen: Array.isArray(evidence.flags_seen)
        ? evidence.flags_seen.length
        : 0,
      prompt_tokens: Number(evidence.usage?.prompt_tokens || 0),
      completion_tokens: Number(evidence.usage?.completion_tokens || 0),
      cached_tokens: Number(evidence.usage?.cached_tokens || 0),
      total_tokens: Number(evidence.usage?.total_tokens || 0),
      cost_usd: Number(evidence.usage?.cost_usd || 0),
    },
    transcript,
    runtime,
    shell_listener: shellListener,
    issues,
  };
}

// ---------------------------------------------------------------------------
// Full report builder
// ---------------------------------------------------------------------------

export function buildReport(runsDir) {
  const entries = loadAssessedEntries(runsDir);
  const reports = entries
    .map((e) => summarizeRunKpi(path.join(runsDir, e.stamp), e.evidence))
    .sort((a, b) => a.stamp.localeCompare(b.stamp));
  const latestByTask = selectNewestEvidence(entries);
  const validByTask = selectNewestValidEvidence(entries);
  const latestReports = [...latestByTask.values()]
    .map((e) => summarizeRunKpi(path.join(runsDir, e.stamp), e.evidence))
    .sort((a, b) => a.task.localeCompare(b.task));
  const validReports = [...validByTask.values()]
    .map((e) => summarizeRunKpi(path.join(runsDir, e.stamp), e.evidence))
    .sort((a, b) => a.task.localeCompare(b.task));

  const issueSummary = {};
  for (const r of latestReports)
    for (const i of r.issues)
      issueSummary[i] = (issueSummary[i] || 0) + 1;

  const validDurations = validReports.map((r) => r.benchmark.duration_s);
  const toolCalls = reports.reduce(
    (s, r) => s + r.runtime.tool_call_count,
    0,
  );
  const totals = {
    run_count: reports.length,
    latest_task_count: latestReports.length,
    valid_task_count: validReports.length,
    solved_valid_task_count: validReports.filter((r) => r.benchmark.solved)
      .length,
    excluded_latest_task_count: latestReports.filter(
      (r) => !r.benchmark.valid_for_score,
    ).length,
    runtime_tool_failures: reports.reduce(
      (s, r) => s + r.runtime.tool_failure_count,
      0,
    ),
    tool_call_count: toolCalls,
    tool_failure_rate_pct: toolCalls
      ? Number(
          (
            (100 *
              reports.reduce(
                (s, r) => s + r.runtime.tool_failure_count,
                0,
              )) /
            toolCalls
          ).toFixed(2),
        )
      : 0,
    solve_rate_pct: 0,
    duration_p50_s: percentile(validDurations, 0.5),
    duration_p95_s: percentile(validDurations, 0.95),
    total_tokens: validReports.reduce(
      (s, r) => s + r.benchmark.total_tokens,
      0,
    ),
    total_elapsed_s: reports.reduce(
      (s, r) => s + r.benchmark.duration_s,
      0,
    ),
    total_turn_count: reports.reduce(
      (s, r) => s + r.runtime.response_count,
      0,
    ),
    average_elapsed_per_turn_s: 0,
    tokens_per_turn: 0,
    provider_wait_total_ms: reports.reduce(
      (s, r) => s + r.runtime.provider_wait_total_ms,
      0,
    ),
    nonzero_shell_exit_sessions: reports.reduce(
      (s, r) => s + r.shell_listener.nonzero_exit_session_count,
      0,
    ),
  };
  totals.solve_rate_pct = totals.valid_task_count
    ? Number(
        (
          (100 * totals.solved_valid_task_count) /
          totals.valid_task_count
        ).toFixed(2),
      )
    : 0;
  totals.average_elapsed_per_turn_s = totals.total_turn_count
    ? Number((totals.total_elapsed_s / totals.total_turn_count).toFixed(2))
    : 0;
  totals.tokens_per_turn = totals.total_turn_count
    ? Number(
        (
          reports.reduce((s, r) => s + r.benchmark.total_tokens, 0) /
          totals.total_turn_count
        ).toFixed(2),
      )
    : 0;

  return {
    totals,
    issue_summary: issueSummary,
    latest_reports: latestReports,
    valid_reports: validReports,
    all_runs: reports,
  };
}

// ---------------------------------------------------------------------------
// Markdown renderer
// ---------------------------------------------------------------------------

export function renderMarkdown(report) {
  const fmtDur = (s) => {
    const v = Math.max(0, Math.round(Number(s) || 0));
    const m = Math.floor(v / 60);
    const r = v % 60;
    return m ? `${m}m ${r}s` : `${r}s`;
  };
  const t = report.totals;
  const lines = [
    "# XBOW-104 KPI Report",
    "",
    `- Total runs observed: **${t.run_count}**`,
    `- Latest task attempts: **${t.latest_task_count}**`,
    `- Valid task attempts for score: **${t.valid_task_count}**`,
    `- Solved valid tasks: **${t.solved_valid_task_count}**`,
    `- Solve rate: **${t.solve_rate_pct}%**`,
    `- Excluded latest attempts: **${t.excluded_latest_task_count}**`,
    `- Runtime tool failures: **${t.runtime_tool_failures}**`,
    `- Tool failure rate: **${t.tool_failure_rate_pct}%**`,
    `- Total elapsed time: **${fmtDur(t.total_elapsed_s)}**`,
    `- Total agent turns: **${t.total_turn_count}**`,
    `- Average elapsed per turn: **${t.average_elapsed_per_turn_s}s**`,
    `- Tokens per turn: **${t.tokens_per_turn}**`,
    `- Valid duration p50 / p95: **${t.duration_p50_s}s / ${t.duration_p95_s}s**`,
    `- Valid total tokens: **${t.total_tokens}**`,
    `- Provider wait total: **${t.provider_wait_total_ms} ms**`,
    `- Non-zero shell exits: **${t.nonzero_shell_exit_sessions}**`,
    "",
    "## Latest Task KPI",
    "",
    "| Task | Outcome | Valid | Duration | Turns | Avg turn | Tok/turn | Final tokens | Tool failures | Shell sessions | Issues |",
    "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
    ...report.latest_reports.map((r) =>
      [
        r.task,
        r.benchmark.outcome,
        r.benchmark.valid_for_score ? "yes" : "no",
        fmtDur(r.benchmark.duration_s),
        r.runtime.response_count,
        r.runtime.response_count
          ? `${(r.benchmark.duration_s / r.runtime.response_count).toFixed(2)}s`
          : "-",
        r.runtime.response_count
          ? (r.benchmark.total_tokens / r.runtime.response_count).toFixed(2)
          : "-",
        r.benchmark.total_tokens,
        r.runtime.tool_failure_count,
        r.shell_listener.session_count,
        r.issues.join(", ") || "-",
      ]
        .join(" | ")
        .replace(/^/, "| ")
        .concat(" |"),
    ),
    "",
    "## Issue Summary",
    "",
    "| Issue | Latest task count |",
    "| --- | --- |",
    ...Object.entries(report.issue_summary)
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([issue, count]) => `| ${issue} | ${count} |`),
    "",
  ];
  return lines.join("\n");
}

// ---------------------------------------------------------------------------
// CLI entry
// ---------------------------------------------------------------------------

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const report = buildReport(RUNS_DIR);
  fs.mkdirSync(path.dirname(KPI_JSON_FILE), { recursive: true });
  fs.mkdirSync(path.dirname(KPI_MD_FILE), { recursive: true });
  fs.writeFileSync(
    KPI_JSON_FILE,
    JSON.stringify(report, null, 2) + "\n",
  );
  fs.writeFileSync(KPI_MD_FILE, renderMarkdown(report));
  console.log(
    `KPI written: ${report.totals.valid_task_count} valid / ${report.totals.latest_task_count} latest tasks`,
  );
}
