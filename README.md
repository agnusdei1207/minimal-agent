<div align="center">

# minimal-agent

**A small, autonomous team-agent runtime — local-first, fast, written in Rust.**

One main agent, up to nine workers, one durable journal. No RAG, no control plane,
no permission engine, no evidence graph, no separate verification service.

[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-000000?logo=rust)](https://www.rust-lang.org/)

</div>

---

## What it is

`minimal-agent` gives a model a real team instead of a single chat loop. **Main**
sets direction and may create, assign, recall, and stop **workers**; workers act on
their assignment and message main and each other directly. Every agent keeps its own
model context and one self-owned `brief.md`; nobody edits another agent's brief.

State lives in two layers and nothing more:

- a bounded, segmented **journal** — the append-only ledger of raw events, used to
  recover a run after a restart;
- each agent's curated **brief** — the current meaning, compacted by the model itself
  under a coverage check, never by rule-based truncation.

That is the whole idea: near-real-time collaboration, memory that stays meaningful
without exploding, and a lifecycle that can lose a turn but never the source.

## 🧭 Philosophy

| Principle | What it means here |
| :--- | :--- |
| **Smallest thing that works** | A new abstraction is added only when the existing pieces genuinely cannot express it. A bounded three-level team keeps delegation small and explicit. |
| **Leverage the model, don't cage it** | Strategy is the model's job, not a rule engine's. The runtime enforces data-loss prevention and resource caps as invariants — never a hardcoded playbook. As models improve, the framework, not the prompt tricks, is what has to keep up. |
| **Make the loops observable** | Good systems start with honest self-observation. Reasoning, tool calls, team messages, and faults are surfaced as distinct, typed transcript lines so you can watch how the team perceives, decides, and recovers. |
| **Divide and conquer, then fan in** | Main decomposes a mission into worker assignments that run in parallel and report back through direct messages; important insights reach main exactly once. Depth is fixed at two so orchestration never recurses out of control. |
| **Every dead end is a gradient** | A failed attempt is signal, not waste. Agents curate facts, dead ends, and next moves into their brief, then pivot instead of repeating an equivalent failed action — iterating toward the objective. |
| **Evidence over claims** | Nothing is "done" because the model said so. A flag counts only when it matches real target output; a result counts only when a tool or test produced it. Completion is recorded through explicit tool results, and heavy verification runs only in resource-capped Docker. |

> I think of it the way I think about playing the piano: polyphony and homophony
> at once. Each voice sings from its own place, and together they still make one
> line. A team of agents is not so different — the orchestration is the music.

## Authorized security work

`minimal-agent` is built to run inside authorized engagements (CTF, labs, pentests).
An engagement's target, scope, off-limits list, and flag format can be injected from
outside — a JSON file or CLI flags — and are rendered into every agent's prompt as
standing authorization, so the team just does the task instead of hedging. See
[`docs/adr/ADR-0002-authorized-engagement-and-transcript-orchestration.md`](docs/adr/ADR-0002-authorized-engagement-and-transcript-orchestration.md)
and [`docs/design/orchestration.md`](docs/design/orchestration.md).

```bash
# Inject an engagement and run autonomously to a captured flag (xbow-style):
minimal-agent run --headless --auto \
  --engagement-kind ctf --target "10.10.10.5" \
  --flag-format 'flag\{[^}]+\}' \
  --goal "Capture the flag on the target"
```

A flag is only ever taken from real target output — the runtime never invents one.
Benchmark execution and scoring stay with an external owner; this repository provides
the injection and autonomous-run interface, and claims no benchmark result.

## Requirements

- An OpenAI-compatible chat-completions endpoint.

Run the TUI and use `/model` to enter the API key, base URL, model name, and
context-token limit in that order. The setup flow keeps credentials out of the
transcript and persists the active provider under the state directory.
`OPENAI_API_KEY`, `OPENAI_MODEL`, and `OPENAI_BASE_URL` remain optional bootstrap
inputs. Rust is pinned to stable `1.98.0`; the npm launcher uses Node.js 24 LTS.

## Run

```bash
minimal-agent run --goal "Complete the task" --workspace .
```

An interactive terminal opens the TUI. Piped input automatically uses plain mode:

```bash
printf 'Inspect the workspace\n' | minimal-agent run --plain --goal "Review the project"
```

Resume or inspect a durable run:

```bash
minimal-agent run --resume .minimal-agent/runs/<run-id> --workspace .
minimal-agent inspect --run .minimal-agent/runs/<run-id>
```

### While the team works

- **Steer without restarting.** Type while main is working and your input folds into
  the running turn at the next safe boundary — the agent keeps its context and
  re-prioritizes around the new instruction instead of losing progress.
- **Interrupt.** Esc cancels the active main turn outright; Ctrl+C clears the editor.
- **Scroll.** The transcript is top-anchored: a scrolled-up view stays put while new
  output streams in at the bottom. Scroll with the arrow keys or PageUp/PageDown;
  Ctrl+End returns to following the newest line. The mouse is not captured, so
  click-drag text selection and copy work normally.

## Commands

The command surface is intentionally small — a sprawling list just goes unused:

`/compact`, `/new` (`/clear`), `/goal`, `/auto`, `/model`, `/status`,
`/resume`, `/agent`, `/agent-<id>`, `/update`, `/help`, `/exit`, and `!<command>`.

`/goal` only records the objective; `/auto` alone stops or resumes autonomous work.
`/compact`, `/goal`, ordinary messages, and shell shortcuts share the main FIFO.

## Bounds

The hot path is bounded on purpose: active team 10, unread Inbox 64 messages / 64 KiB
per agent, input 64 KiB, display transcript 2 MiB, and tool arguments/results plus
shell/file/journal reads 128 KiB. Limits reject new work without silently deleting or
truncating durable source data. Bracketed paste is all-or-nothing at the input limit.
The TUI runs on a full-height alternate screen so the header stays put and your
shell scrollback is untouched. The mouse is not captured, so native text
selection keeps working; scroll the transcript with the arrow / PageUp-PageDown keys.

## npm

Current release: `0.110.0`.

```bash
npm install --global minimal-agent@0.110.0
```

The install script downloads the matching native release asset and verifies its
SHA-256 from the release checksum file before installation. Downloads use a
120-second deadline with a 128 MiB asset limit and a 1 MiB checksum-file limit; the
npm pack verifier permits only the launcher, installer, bounded-download helper,
license, README, and package manifest.

## Docker

```bash
npm run check
docker compose -f docker/compose.yaml run --rm minimal-agent
```

`npm run check` builds the owned Ubuntu 26.04 runtime base, including pinned Chrome
for Testing and the command-line toolset, then builds the app image and opens the TUI
in a 2 GiB / 2 CPU / PID-capped container. Its workspace and durable state use the
`minimal-agent-workspace` and `minimal-agent-state` named volumes.

The app image runs as non-root UID/GID `10001:10001`. The agent's shell and file
tools operate with the permissions of the process or container; use an appropriately
isolated workspace for untrusted tasks.

## Development

Rust compilation and tests are Docker-only and resource-capped:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dbuild.ps1 test --all-targets
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dbuild.ps1 clippy --all-targets -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/nverify.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dimage.ps1
```

The architecture decisions and executable requirements live in
[`docs/adr/`](docs/adr/): ADR-0001 (minimal team-agent core) and ADR-0002
(authorized engagement, steering, and transcript orchestration).

## License

MIT
