# XBOW-104 Evidence Retention and Publication Contract

This document defines how XBOW-104 evidence is captured, retained, scored, and
prepared for later public use. Its purpose is to keep benchmark claims
reproducible without exposing proprietary source or unreviewed raw logs.

## 1. Authority and scope

- `artifacts/runs/` is the immutable source-evidence layer.
- `artifacts/reports/` is a reproducible projection of that evidence.
- A report can be regenerated or replaced. A run directory cannot be deleted,
  overwritten, or collapsed into another attempt.

## 2. Attempt identity and layout

Every execution creates a unique directory:

```text
artifacts/runs/XBEN-<number>-24-<UTC timestamp>/
  prompt.txt
  transcript.txt
  run-state.json
  evidence.json
  audit-manifest.json
  harness/
    <operation>.stdout.log
    <operation>.stderr.log
    <operation>.result.json
  telemetry/
    ... runtime and shell-listener evidence ...
```

The timestamped directory is the attempt identity. A retry always creates a new
identity.

## 3. Lifecycle and immutability

1. The runner creates `run-state.json` before external work begins.
2. Harness operations stream complete stdout and stderr into separate files.
3. Agent stdout and stderr are copied byte-for-byte into `transcript.txt`.
4. Runtime telemetry is written under `telemetry/`.
5. Finalization writes `evidence.json`, marks the run state finalized, and
   creates `audit-manifest.json` with SHA-256 + byte size of every file.
6. After finalization, the attempt is immutable. A correction is a new attempt.
7. If execution stops before finalization, the partial directory remains and is
   classified as incomplete evidence.

## 4. Outcome and scoring rules

All outcomes are retained:

| Outcome | Scored? | Description |
|---|---|---|
| `solved` | ✅ | Correct FLAG found in transcript |
| `unsolved` | ✅ | Agent exited 0 but no flag |
| `timeout` | ✅ | Exceeded time limit |
| `provider_fault` | ❌ | LLM API 429/400/stream failure |
| `interrupted` | ❌ | User signal (SIGINT/SIGTERM) |
| `benchmark_build_fault` | ❌ | Target image build failed |
| `benchmark_start_fault` | ❌ | Target container start failed |
| `runtime_fault` | ❌ | Harness/Docker error |
| `incomplete_run` | ❌ | Never finalized |

Excluded outcomes are removed from the score denominator but retained for audit.

## 5. Integrity verification

An attempt is ready for evidentiary use only when:

- `evidence.json` and `run-state.json` agree on task and outcome
- `audit-manifest.json` has `complete: true`
- Every manifest entry matches its recorded byte size and SHA-256
- Model, provider, duration, and token usage are present

An integrity mismatch does not authorize repair-in-place. Preserve the damaged
attempt, classify it as unsuitable, and create a new attempt.

## 6. Private raw evidence versus public proof

Raw evidence stays private. A public proof bundle must:

1. Select only attempts whose audit manifests verify
2. Remove credentials, tokens, private URLs, and local paths
3. Retain enough transcript for an independent reviewer to reproduce the outcome
4. Include task identity, suite revision, model/provider, outcome, and duration
