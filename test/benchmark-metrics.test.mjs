import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { classifyAttempt, loadAssessedEntries, selectNewestValidEvidence } from '../benchmarks/harness/evidence.mjs';
import { summarizeRunKpi, buildReport } from '../benchmarks/harness/kpi.mjs';
import { loadMinimalAgentRows, renderStandardReport } from '../benchmarks/harness/lib/standard-report.mjs';

function fixture(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'benchmark-metrics-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  return dir;
}
function attempt(root, stamp, data, usage) {
  const dir = path.join(root, stamp);
  fs.mkdirSync(path.join(dir, 'telemetry'), { recursive: true });
  fs.writeFileSync(path.join(dir, 'evidence.json'), JSON.stringify({ task: 'XBEN-020-24', ...data }));
  if (usage) fs.writeFileSync(path.join(dir, 'telemetry/usage.jsonl'), usage.map(r => JSON.stringify(r)).join('\n'));
  return dir;
}

test('recovered provider retries do not exclude completed unsolved attempts', () => {
  assert.deepEqual(classifyAttempt({ exitCode: 0, providerRateLimitCount: 2 }), { outcome: 'unsolved', validForScore: true });
  assert.deepEqual(classifyAttempt({ timedOut: true, exitCode: 124, providerStreamFailureCount: 1 }), { outcome: 'timeout', validForScore: true });
  assert.deepEqual(classifyAttempt({ exitCode: 1, providerStreamFailureCount: 1 }), { outcome: 'provider_fault', validForScore: false });
});

test('retention preserves later failures and in-progress attempts instead of keeping only successes', t => {
  const root = fixture(t);
  attempt(root, 'XBEN-020-24-2026-01-01', { solved: true, valid_for_score: true, outcome: 'solved' });
  attempt(root, 'XBEN-020-24-2026-01-02', { solved: false, valid_for_score: true, outcome: 'unsolved' });
  const live = path.join(root, 'XBEN-020-24-2026-01-03');
  fs.mkdirSync(live);
  fs.writeFileSync(path.join(live, 'run-state.json'), JSON.stringify({ task: 'XBEN-020-24', phase: 'agent' }));
  loadAssessedEntries(root);
  assert.equal(fs.readdirSync(root).length, 3);
  assert.equal(selectNewestValidEvidence(loadAssessedEntries(root)).get('XBEN-020-24').evidence.solved, false);
});

test('KPI reads current usage telemetry and derives total tokens without fabricating tool counts', t => {
  const root = fixture(t);
  const evidence = { solved: false, valid_for_score: true, outcome: 'unsolved', usage: { prompt_tokens: 100, completion_tokens: 20, total_tokens: 0 } };
  const dir = attempt(root, 'XBEN-020-24-2026-01-01', evidence, [
    { event: 'response', prompt_tokens: 40, completion_tokens: 8 },
    { event: 'response', prompt_tokens: 60, completion_tokens: 12 },
  ]);
  const r = summarizeRunKpi(dir, evidence);
  assert.equal(r.runtime.response_count, 2);
  assert.equal(r.runtime.response_total_tokens, 120);
  assert.equal(r.benchmark.total_tokens, 120);
  assert.equal(r.runtime.tool_call_count, null);
  assert.equal(buildReport(root).totals.tool_failure_rate_pct, null);
  assert.equal(r.shell_listener.session_count, null);
  assert.equal(buildReport(root).totals.nonzero_shell_exit_sessions, null);
});

test('KPI tolerates a partially written shell snapshot and does not duplicate logged usage', t => {
  const root = fixture(t);
  const dir = attempt(root, 'XBEN-020-24-2026-01-01', { valid_for_score: true, outcome: 'unsolved' }, [{ event: 'response', prompt_tokens: 40, completion_tokens: 8 }]);
  fs.mkdirSync(path.join(dir, 'telemetry/logs'));
  fs.writeFileSync(path.join(dir, 'telemetry/logs/runtime.log'), JSON.stringify({ event: 'response', prompt_tokens: 40, completion_tokens: 8 }));
  fs.mkdirSync(path.join(dir, 'telemetry/shell-listener'));
  fs.writeFileSync(path.join(dir, 'telemetry/shell-listener/sessions.snapshot.json'), '{');
  const r = summarizeRunKpi(dir, {});
  assert.equal(r.runtime.response_count, 1);
  assert.equal(r.runtime.response_total_tokens, 48);
  assert.equal(r.shell_listener.session_count, null);
});

test('summary and index derive totals and preserve selected attempt provenance', t => {
  const root = fixture(t);
  const runs = path.join(root, 'runs');
  attempt(runs, 'XBEN-020-24-2026-01-01', { solved: false, valid_for_score: true, outcome: 'unsolved', usage: { prompt_tokens: 1000, completion_tokens: 2000, total_tokens: 0 } });
  for (const script of ['summary.mjs', 'build-results-index.mjs']) {
    const result = spawnSync(process.execPath, [`benchmarks/harness/${script}`], { encoding: 'utf8', env: { ...process.env, XBOW104_RUNS_DIR: runs, XBOW104_REPORTS_DIR: root } });
    assert.equal(result.status, 0, result.stderr);
  }
  const index = JSON.parse(fs.readFileSync(path.join(root, 'results-index.json'), 'utf8'));
  assert.equal(index.tasks[0].usage.total_tokens, 3000);
  assert.equal(index.tasks[0].stamp, 'XBEN-020-24-2026-01-01');
  assert.match(fs.readFileSync(path.join(root, 'SUMMARY.md'), 'utf8'), /total \*\*3k\*\*/);
});

test('standard report excludes excluded solved rows from score numerator', t => {
  const reportsDir = fixture(t);
  const s = renderStandardReport({ model: { slug: 'fixture', id: 'fixture', label: 'fixture' }, rows: [
    { task: 'XBEN-001-24', valid_for_score: true, solved: true },
    { task: 'XBEN-002-24', valid_for_score: false, solved: true },
  ], reportsDir, generator: 'test', header: { title: '# Fixture', solverLine: 'Fixture' } });
  assert.equal(s.solveRate, (1 / 104) * 100);
  assert.equal(s.solved, 1);
  const kpi = JSON.parse(fs.readFileSync(path.join(reportsDir, 'kpi.json'), 'utf8'));
  assert.equal(kpi.averages.per_task.turns, null);
  assert.equal(kpi.averages.tokens_per_turn_in_plus_out, null);
  assert.match(kpi.totals.note_infra_excluded, /includes setup.*teardown.*harness/i);
  const markdown = fs.readFileSync(path.join(reportsDir, 'SUMMARY.md'), 'utf8');
  assert.match(markdown, /Recorded attempt elapsed time/);
  assert.doesNotMatch(markdown, /solver wall time|infra.*excluded/i);
});

test('minimal adapter only counts response usage and does not count cached prompt tokens twice', t => {
  const root = fixture(t);
  attempt(root, 'XBEN-020-24-2026-01-01', { solved: false, valid_for_score: true, outcome: 'unsolved' }, [
    { event: 'request_start' },
    { event: 'response', prompt_tokens: 100, completion_tokens: 20, cached_tokens: 60 },
  ]);
  const rows = loadMinimalAgentRows(root);
  assert.equal(rows[0].num_turns, 1);
  assert.equal(rows[0].input, 40);
  assert.equal(rows[0].cache_read, 60);
});

test('standard consumption includes retries while score uses one finalized attempt per task', t => {
  const root = fixture(t);
  attempt(root, 'XBEN-020-24-2026-01-01', { solved: false, valid_for_score: true, outcome: 'unsolved' }, [{ event: 'response', prompt_tokens: 100, completion_tokens: 20 }]);
  attempt(root, 'XBEN-020-24-2026-01-02', { solved: true, valid_for_score: true, outcome: 'solved' }, [{ event: 'response', prompt_tokens: 200, completion_tokens: 30 }]);
  const rows = loadMinimalAgentRows(root);
  const attempts = loadMinimalAgentRows(root, { allAttempts: true });
  assert.equal(rows.length, 1);
  assert.equal(attempts.length, 2);
  const reportsDir = path.join(root, 'reports');
  const s = renderStandardReport({ model: { slug: 'fixture', id: 'fixture', label: 'fixture' }, rows, attempts, reportsDir, generator: 'test', header: { title: '# Fixture', solverLine: 'Fixture' } });
  assert.equal(s.solved, 1);
  assert.equal(s.tot.in_plus_out, 350);
  const kpi = JSON.parse(fs.readFileSync(path.join(reportsDir, 'kpi.json'), 'utf8'));
  assert.equal(kpi.totals.attempt_count, 2);
  assert.equal(kpi.attempts.length, 2);
});

test('zai summary uses valid score numerator and retains retry consumption', t => {
  const root = fixture(t);
  const zaiDir = path.join(root, 'benchmarks/zai');
  const lib = path.join(root, 'benchmarks/harness/lib');
  const harnessDir = path.join(root, 'benchmarks/harness');
  fs.mkdirSync(zaiDir, { recursive: true });
  fs.mkdirSync(lib, { recursive: true });
  fs.mkdirSync(harnessDir, { recursive: true });
  fs.copyFileSync('benchmarks/zai/summarize.mjs', path.join(zaiDir, 'summarize.mjs'));
  fs.copyFileSync('benchmarks/harness/lib/standard-report.mjs', path.join(lib, 'standard-report.mjs'));
  fs.copyFileSync('benchmarks/harness/control.mjs', path.join(harnessDir, 'control.mjs'));
  const artifacts = path.join(zaiDir, 'glm-5.3-flash/artifacts');
  const runs = path.join(artifacts, 'runs');
  attempt(runs, 'XBEN-020-24-2026-01-01', { solved: false, valid_for_score: true, usage: { prompt_tokens: 100, completion_tokens: 20 } });
  attempt(runs, 'XBEN-020-24-2026-01-02', { solved: true, valid_for_score: true, usage: { prompt_tokens: 200, completion_tokens: 30 } });
  attempt(runs, 'XBEN-021-24-2026-01-01', { task: 'XBEN-021-24', solved: true, valid_for_score: false });
  const result = spawnSync(process.execPath, [path.join(zaiDir, 'summarize.mjs'), '--model', 'glm-5.3-flash'], {
    cwd: root,
    encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stderr);
  const kpi = JSON.parse(fs.readFileSync(path.join(artifacts, 'reports/kpi.json'), 'utf8'));
  assert.equal(kpi.totals.solve_rate_pct, Number(((1 / 104) * 100).toFixed(1)));
  assert.equal(kpi.totals.tokens.in_plus_out, 350);
  assert.equal(kpi.totals.attempt_count, 3);
  assert.equal(kpi.totals.cost_usd, null);
});
