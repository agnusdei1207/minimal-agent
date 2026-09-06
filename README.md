<div align="center">

# minimal-agent

**A small, autonomous team-agent runtime — local-first, fast, written in Rust.**

One main agent, up to nine workers, one durable journal. No RAG, no control plane,
no permission engine, no evidence graph, no separate verification service.

[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-000000?logo=rust)](https://www.rust-lang.org/)

</div>

---

## 🎯 Purpose

`minimal-agent` gives a language model a real, autonomous team instead of a single conversational chat loop.

- **Main Agent (Depth 0):** Decomposes the mission, assigns bounded tasks, recalls workers, and synthesizes progress.
- **Worker Agents (Depth 1):** Up to 9 parallel workers executing discrete tasks and messaging Main and siblings directly.
- **Durable Run Journal:** A segmented, append-only ledger of raw events for zero-data-loss crash recovery.
- **Self-Owned Briefs:** Every agent curates its own `brief.md` (current battlefield understanding) via semantic compaction.

---

## 🧭 Design Philosophy

1. **Simple is Best (curl philosophy):** The smallest thing that works. A lean, sharp kernel that scales through composable CLI primitives, not monolithic bloat.
2. **Prompt/Skill-First (Zero Code Bloat):** Never solve in code what can be solved in prompts or skills. Code is a permanent maintenance liability. The Rust runtime strictly enforces OS/IO invariants (`bash`, resource caps, process reaping), while domain tradecraft lives in modular skill prompts.
3. **Bounded Team Hierarchy:** Fixed depth of 2 (Main $\to$ Workers) prevents runaway recursive spawning and keeps token budgets strictly bounded.
4. **Evidence Over Claims:** Nothing is "done" simply because the model claims so. Completions, vulnerabilities, and flags require concrete tool output or executable verification.

---

## ⚡ Quick Start

### 1. Environment Setup (`.env`)

Create a `.env` file in the project root:

```bash
OPENAI_API_KEY="your-api-key"
OPENAI_MODEL="your-model-name"                # e.g., glm-5.3-flash, gpt-4o, claude-3-5-sonnet
OPENAI_BASE_URL="https://api.openai.com/v1"  # optional (supports any OpenAI-compatible endpoint)
# OPENROUTER_API_KEY="your-key"               # optional
```

### 2. Run via Docker

```bash
# Option A: Docker Compose (Recommended)
docker compose -f docker/compose.yaml run --rm minimal-agent

# Option B: Docker One-Shot CLI
docker run --rm -it --init \
  --cap-add=NET_RAW --cap-add=NET_ADMIN \
  --env-file .env \
  -v ${PWD}/workspace:/workspace \
  -v ${PWD}/runs:/state \
  agnusdei1207/minimal-agent:0.110.0
```

### 3. Run via npm

Current release: `0.110.0`.

```bash
# Option A: Build and launch isolated Docker TUI
npm run check

# Option B: Global CLI installation
npm install --global minimal-agent@0.110.0
minimal-agent run --goal "Investigate the target and solve the objective" --workspace .
```

---

## 📄 License

MIT
