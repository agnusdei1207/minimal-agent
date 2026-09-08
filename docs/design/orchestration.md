# Minimal Team-Agent Orchestration for Autonomous Penetration Testing

This document presents the orchestration architecture of `minimal-agent` in a format suitable for academic citation and technical presentations. Normative specifications reside in [INTENT-0001](../intents/0001-minimal-autonomous-team-agent-core.md) (Core Runtime) and [INTENT-0002](../intents/0002-authorized-engagement-and-transcript-orchestration.md) (Engagement Injection & Transcript Orchestration); this document connects those architectural decisions into a cohesive narrative.

---

## 1. Problem Definition

Autonomous offensive security agents must withstand three simultaneous systemic pressures:

1. **Concurrent Collaboration:** Reconnaissance, vulnerability identification, and exploit construction proceed in parallel and must share real-time discoveries without communication bottlenecks.
2. **Context Retention (Memory Pressure):** In long-horizon engagements, conversation context windows explode. The agent must maintain current operational meaning without losing the immutable ground truth of prior actions.
3. **Fault Recovery:** The runtime must remain recoverable from its original append-only journal across provider outages, rate limits, and partial worker crashes.

Conventional multi-agent frameworks attempt to absorb these pressures by layering external databases, RAG systems, blackboard architectures, complex permission engines, and shared memory stores. In contrast, `minimal-agent` **minimizes conceptual primitives**, evaluating its success solely on whether each operational boundary holds reliably under saturation, interruption, and restart.

---

## 2. The Orchestration Model

### 2.1 Bounded Team Topology

```text
main (depth 0)  ── Goal interpretation, team formulation, strategic synthesis
├─ worker-01 (depth 1)  ── Reconnaissance & network scanning
├─ worker-02 (depth 1)  ── Web vulnerability probing
└─ ... up to 9 workers   ── Lateral exploitation and privilege escalation
```

- Only `main` creates, assigns, steers, and recalls workers. Workers cannot spawn arbitrary child subagents.
- Maximum active team size is strictly capped at 10 (including `main`). Roles and assignments are determined dynamically at runtime.
- Deep recursive sprawl is prevented by fixing team topology bounds. Rather than adding arbitrary hierarchical depth, `minimal-agent` prioritizes controllable, low-latency coordination. Bounded 3-depth extensions in complex operations are detailed in Section 2.5 and [INTENT-0004](../intents/0004-bounded-three-depth-hierarchical-orchestration.md).

### 2.2 Direct Messaging over a Single Append-Only Journal

Agents do not coordinate through a shared message board; they exchange typed messages through a single durable journal:

- **Routes:** `main → worker`, `worker → main`, `worker → worker`, and multiple recipients.
- **Message Kinds:** `Progress`, `Insight`, `Request`, and `Final`. `Insight` and `Final` automatically include `main` as an implicit recipient, eliminating redundant duplicate broadcasts.
- **Durable Projection:** Every message is appended once to the run journal and projected directly into the recipient's in-memory Inbox. A process restart folds the same journal to restore all unconsumed inbox messages.

This single-journal, direct-communication model is the core of the runtime. Communication, state tracking, and crash recovery all derive from the same append-only record of historical facts, removing the need for a secondary control or observation plane.

### 2.3 Owned Memory (`brief.md`)

Every agent maintains an individually owned battlefield note (`brief.md`):
- `main` maintains the global picture, team composition, active attack frontiers, and curated technical insights.
- No agent directly modifies another agent's semantic brief.
- When an individual agent's context approaches its threshold (~80%), only that specific agent undergoes LLM semantic compaction.

### 2.4 Inter-Agent Messaging: Underlying LLM Array Transformations

At the LLM API layer, inter-agent communication is an append operation to the recipient's private conversation array (`messages: [...]`):

```text
[Agent A's Conversation Array]                 [Agent B's Conversation Array]
┌────────────────────────────┐                 ┌────────────────────────────┐
│ system : Role directives   │                 │ system : Role directives   │
│ user   : Task instruction  │                 │ user   : Task instruction  │
│ assistant: [Tool Call] ──┐ │ (Extract body)  │ assistant: Previous turn   │
│ tool   : Delivery confirmed│ │               │ tool   : Tool result       │
└────────────────────────────┘ └──────────────>│ user   : [Team Message]    │ <-- Appended to B's
                                               │   "A: Port 80 found"       │     array tail!
                                               └────────────────────────────┘
```

1. Sender invokes `team` tool with `op: "send"`.
2. Sender receives a delivery confirmation tool result (`"Message delivered to worker-02"`).
3. Central runtime intercepts the body and appends it as a `user` role message to the recipient's array on its next turn.
4. Each agent maintains an isolated conversation graph, preventing context pollution across roles.

### 2.5 Topology Evolution & Trade-offs: Star vs. Hierarchical Tree

| Dimension | Star Topology (Active: Depth 1) | Hierarchical Tree (Extended: Depth 2–3) |
| :--- | :--- | :--- |
| **Structure** | Radial star graph (1 Main + ≤9 Workers) | $N$-ary Tree (Root $\to$ Leads $\to$ Leaf Workers) |
| **Tool Rights** | Main + Workers | Strictly Leaf workers only |
| **Strengths** | Minimum latency, zero multi-hop summarization loss | High context purity for supervisors; scales beyond 10 agents |
| **Risks** | Coordinator span-of-control saturation at >10 agents | Value degradation across hops, turn latency explosion, spawn storms |

For focused web penetration testing and CTFs (e.g., XBOW-104), Star Topology ($K_{1, n}$) minimizes latency and preserves exact exploit artifacts. Bounded tree structures (INTENT-0004) are reserved for large-scale multi-track operations.

---

## 3. The Engagement Layer (INTENT-0002)

A bare goal string is insufficient to delineate the boundaries and constraints of an offensive engagement. The `Engagement` domain model formalizes this context:

```text
Engagement { kind, title, scope, off_limits, flag_format, objective }
```

### 3.1 Injection Pipeline

```text
CLI Flags / --engagement JSON
        └─ Engagement Struct
             └─ RuntimeConfig
                  └─ build_system() ── bounded target-context block
                       └─ System prompt of every agent
```

- Command-line flags overlay file-based definitions, with CLI arguments taking absolute precedence.
- Engagement text fields are strictly bounded by size limits; credentials and secrets are excluded from standard system prompt rendering.
- An `EngagementSet` event is appended to the journal upon run creation, ensuring that session resumptions reliably restore the exact scope and rules of engagement.

### 3.2 Operating Doctrines

- **Standing Authorization Doctrine:** Legitimizes the operational scope as pre-authorized work. Agents are forbidden from emitting moralizing preambles, legal disclaimers, or refusing technical actions (recon, exploit creation, payload generation). Boundaries are operational (scope adherence, evidence preservation, avoiding needless destruction), not legalistic.
- **CTF Solve-Loop Doctrine (`kind == ctf`):** Enforces structured reconnaissance, ranked falsifiable hypotheses, minimal diagnostic probes, strict rabbit-hole bans, and flag extraction solely from verified target output matching `flag_format`.

These doctrines are implemented purely as compact system prompt text rather than complex external policy engines, aligning with INTENT-0001's mandate that strategy should not be hardcoded into the runtime core.

### 3.3 Autonomous Headless Execution (Benchmark Mode)

```bash
minimal-agent run \
  --engagement ./engagement.json \
  --auto --plain --headless
```

1. Submits the primary objective once.
2. The autonomous engine drives the team, observing events until `main` reports `op: "final"` or the team settles into an idle state.
3. Outputs a single-line JSON summary containing verified execution telemetry and recovered flags.
4. If a `flag_format` is specified, exits with a non-zero status code if no valid flag was extracted from actual target output.

Flag extraction strictly requires regex matching against genuine target standard output. The repository provides the injection, autonomous execution, and extraction interface; external harness owners manage target provisioning and scoring.

---

## 4. Transcript Orchestration Visibility (INTENT-0002)

To ensure full observability and reproducibility, transcripts render as structured typed entries:

| Entry Type | Terminal Representation |
| :--- | :--- |
| **Tool Calls** | `shell` prefix with dimmed command arguments, followed by execution status and bounded output |
| **Team Messages** | `main → worker-01` with explicit message kind labels (`Insight`, `Progress`) and substantive payload |
| **Model Reasoning** | Dimmed stream without artificial visual rails |
| **Runtime Errors** | Highlighted `ERROR:` indicators with clear diagnostic messages |

Consistent color palettes and CommonMark terminal rendering allow operators to instantly distinguish between execution, results, inter-agent messaging, and internal reasoning across concurrent workers.

---

## 5. Fault Recovery and Safety Boundaries

- **Atomic Tool Turns:** A model tool invocation and its subsequent results form a single atomic group that cannot be separated across compaction or restart cycles.
- **Provider Fault Handling:** Provider failures pause the affected worker in a `Waiting` state rather than killing it. Subsequent user steering, reassignment, or explicit resume calls awaken the worker without triggering unbounded retry storms.
- **Storage Boundaries:** When storage quotas are reached, write operations halt gracefully without corrupting or deleting existing journal history.

---

## 6. Intentionally Omitted Primitives

`minimal-agent` deliberately omits several common framework abstractions:
- Vector databases and external RAG pipelines
- Global shared team scratchpads
- Separate observation and telemetry microservices
- Complex runtime policy and approval engines
- Automated heuristic strategy classifiers
- In-core PTY daemons: delegated to OS tmux; see [INTENT-0003](../intents/0003-shell-execution-and-interactive-primitives.md)
- Embedded browser engines: delegated to CLI and accessibility trees; see [INTENT-0005](../intents/0005-web-browser-automation-and-traffic-interception.md)

The architectural thesis of this project is that an append-only journal paired with individually owned semantic briefs provides complete state representation. New primitives will only be evaluated via formal ADRs if empirical recall failures are demonstrated.

---

## 7. Scope of Claims

- This codebase **makes no direct claims regarding benchmark solve rates.** Its primary objective is to prove the correctness, minimal conceptual footprint, and invariant safety of its orchestration, memory, and recovery boundaries.
- Formal benchmark metrics are measured and published independently by external harness owners.
- Verifiable proof is established through clean-room containerized builds, linting passes (`scripts/dbuild.ps1`), and deterministic contract test suites.
