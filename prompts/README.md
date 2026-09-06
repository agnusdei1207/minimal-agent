# System prompts

The agent's system prompt is assembled from these files, so the wording lives here
as the single source of truth rather than buried in Rust string literals. They are
compiled in with `include_str!` (trimmed) and rebuilt into every agent's prompt by
`build_system` in `src/runtime.rs`.

## Always applied (every agent)

| File | What it does |
| :--- | :--- |
| `authorized-engagement.md` | Establishes the authorized offensive-security operator role. |
| `execution-mandate.md` | **PROTECTED anti-refusal contract** — the one file that stops the model refusing, deflecting, asking the user to paste the problem, or second-guessing the target. Keep it short and forceful; **do NOT weaken or touch it when editing other prompts** (churn here re-triggers weak-model refusals). |
| `operating-style.md` | Terse, action-first execution: no preamble, narration, deliberation, or emoji/decoration. |
| `tradecraft.md` | Points to the on-disk attack-methodology library (`/opt/minimal-agent/skills`) for autonomous consultation, and the shared-session ("public desk") rules for live/interactive terminals. |
| `team-conduct.md` | Team conduct: act with tools, continue until done, curate the brief. |
| `communication.md` | How agents talk: the four message kinds (Progress/Insight/Request/Final), substance-only, `team send`/`team finish` vs. plain text. |
| `team-tree.md` | The bounded 3-depth team tree (ADR-0004): neighbor-only communication, faithful verbatim upward reporting (no telephone game), core-note hygiene, and restart behavior. |

## Role-specific

Assembled from the agent's **position** in the team tree (ADR-0004). main keeps its
three role files; every other agent gets `worker-role.md` plus an internal- or
leaf-node file chosen by whether it currently has children.

| File | Applies to |
| :--- | :--- |
| `main-role.md` | main (depth 0) — decompose the objective, create/assign/recall/stop workers, synthesize. |
| `fan-out.md` | main only — parallelize independent subtasks across workers instead of running one worker serially. |
| `self-management.md` | main only — periodically summarize the battlefield into the brief so working memory stays bounded. |
| `worker-role.md` | a non-main agent (depth 1–2) — act on the assignment and report via team messages. Depth 1 may spawn depth-2 children; depth 2 is a leaf and cannot. |
| `node-internal.md` | a non-main agent that currently has children — decompose, delegate, and aggregate its children's results faithfully upward. |
| `node-leaf.md` | a non-main agent with no children — execute directly and report exact values to its parent. |

A generated **POSITION** block (depth, node kind, parent, children, siblings) is
appended just before the brief so the agent knows exactly where it sits.

## Conditional

| File | When |
| :--- | :--- |
| `ctf-solve-loop.md` | `engagement.kind == ctf` |

## Assembly order

authorized-engagement → execution-mandate → operating-style → tradecraft → (engagement target block, if injected) →
(ctf-solve-loop, if CTF) → team-conduct → communication → team-tree →
(main-role + fan-out + self-management) **or** (worker-role + node-internal **or** node-leaf) →
POSITION block → the agent's current brief.

Editing a file here changes the prompt on the next build — no code change needed.
Keep the text clean-room (authored for this repository).

---

## Operational Philosophy & Prompt Engineering

The system prompt design principles, offensive disciplines, live-seam diagnostic rules, self-reflection framework, and anti-rabbit-hole mechanics are fully articulated in:

👉 [**`docs/design/prompt-philosophy.md`**](../docs/design/prompt-philosophy.md)

### Key Principles at a Glance
1. **Unconditional Direct Action**: Stand upon declared targets; no moralizing, disclaimers, or refusals.
2. **Hypothesis Divergence over Anchoring**: Formulate 3–5 structurally distinct hypotheses before deep diving.
3. **Differential Signal as Oracle**: Treat every probe as a diagnostic sensor; 3–5 identical responses = silent wall -> backtrack.
4. **Live Seam vs. Silent Wall**: Errors, 500 status, and filter blockages prove backend interpreter contact; never abandon live seams.
5. **Orthogonal Creative Bypasses**: Vary context escaping, encodings, delimiters, and functional equivalents; use ES5 for legacy headless bots.
6. **Meta-Cognitive Self-Reflection**: Run loop, evidence, drift, and progress self-audits before executing tool calls.
7. **Time-Sink Bans**: Strict bans on offline dictionary cracking, UI steganography, and heavy local test harnesses.

