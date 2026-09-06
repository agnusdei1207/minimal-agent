<div align="center">

# minimal-agent-pentesting

<a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green.svg" alt="License: MIT"></a>
<a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/built%20with-Rust-000000?logo=rust" alt="Built with Rust"></a>
<a href="benchmarks/deepseek-v4-flash/artifacts/reports/SUMMARY.md"><img src="https://img.shields.io/badge/XBOW--104-98.1%25%20(102%2F104)-C8FF00" alt="XBOW-104 98.1%"></a>
<a href="https://agnusdei1207.github.io/minimal-agent-pentesting/"><img src="https://img.shields.io/badge/Docs-GitHub%20Pages-blue" alt="Docs"></a>

</div>

## 📊 Benchmark — XBOW-104

Empirical evaluation on the **XBOW-104** web exploitation suite (104 single-flag CTFs) under zero human intervention:

<div align="center">

<img src="https://raw.githubusercontent.com/agnusdei1207/minimal-agent-pentesting/main/assets/benchmark_matrix.svg" alt="XBOW-104 Benchmark" width="100%">

</div>

| Model | Solved | Rate | Tokens | Avg Time | Est. Cost | L1 / L2 / L3 Solved |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **DeepSeek-V4-Flash** | **102 / 104** | **98.1%** | **257.0M** | **19.6 min** | **$2.27** | **44/45 · 51/51 · 7/8** |
| **GLM-5.3-Flash** | 94 / 104 | 90.4% | 176.6M | 22.6 min | ~$9 | 44/45 · 44/51 · 6/8 |

> 📄 **[Detailed Benchmark Report (SUMMARY.md)](benchmarks/deepseek-v4-flash/artifacts/reports/SUMMARY.md)** · 🌐 **[Web Dashboard](https://agnusdei1207.github.io/minimal-agent-pentesting/)**

---

## ⚡ Quick Start

### 1. Environment Setup


```bash
# .env

OPENAI_API_KEY="your-api-key"
OPENAI_MODEL="your-model-name"
OPENAI_BASE_URL="https://api.openai.com/v1"  # optional, e.g. https://openrouter.ai/api/v1
OPENAI_CONTEXT_TOKENS="128k"                 # optional context ceiling, e.g. 128k, 1m
OPENAI_MAX_OUTPUT_TOKENS="16k"               # optional max output per turn, e.g. 16k, 32k
```

### 2. Run via Docker

```bash
# Option A: Docker Compose (Recommended)
docker compose --env-file .env -f docker/compose.yaml run --rm minimal-agent

# Option B: Docker CLI — Interactive TUI
docker run --rm -it --init \
  --cap-add=NET_RAW --cap-add=NET_ADMIN \
  --env OPENAI_API_KEY="your-api-key" \
  --env OPENAI_MODEL="your-model-name" \
  -v ${PWD}/workspace:/workspace \
  -v ${PWD}/runs:/state \
  agnusdei1207/minimal-agent-pentesting:latest

# Option C: Docker CLI — One-shot Goal
docker run --rm -it --init \
  --cap-add=NET_RAW --cap-add=NET_ADMIN \
  --env OPENAI_API_KEY="your-api-key" \
  --env OPENAI_MODEL="your-model-name" \
  -v ${PWD}/workspace:/workspace \
  -v ${PWD}/runs:/state \
  agnusdei1207/minimal-agent-pentesting:latest \
  run --goal "Investigate the target and solve the objective" --workspace /workspace --run /state/current
```

### 3. Run via npm

```bash
# Option A: Build and launch isolated Docker TUI
npm run check

# Option B: Global CLI — Interactive TUI
npm install --global minimal-agent-pentesting
minimal-agent-pentesting

# Option C: Global CLI — One-shot Goal
minimal-agent-pentesting run --goal "Investigate the target and solve the objective" --workspace .
```

---

## 🧭 Philosophy & Architecture

`minimal-agent-pentesting` is engineered around a clean-room, zero-bloat offensive security philosophy formalized in [`docs/design/`](docs/design/):

- **Prompt-First over Code-Bloat (Code is Debt)**: Never write Rust code for what prompts and markdown skills can achieve. The lean Rust runtime (`crates/ma-runtime`, `crates/ma-core`) enforces only strict OS/IO invariants (sandboxing, 16KB output truncation, orphan cleanup). Domain tradecraft and reconnaissance hygiene live entirely in on-disk skill cards (`prompts/skills/`).
- **Seams & Tracer Bullets**: Decomposes targets into architectural seams (protocols, auth layers, interpreters, internal container networks). Sends minimal, sharp tracer bullets to capture differential signals (500 errors, WAF blocks, byte deltas) at live seams while immediately backtracking from silent walls.
- **4-D Orthogonal Bypass**: Eliminates the depth-first trap of repetitive syntax guessing by simultaneously varying across four orthogonal axes: context escaping, parser differentials/encodings, functional primitive equivalents, and ES5 universal bot compatibility.
- **Single-Journal, Zero-DB Orchestration**: Eliminates external databases, vector RAG, and blackboard locks. A single append-only event log (`crates/ma-journal`) enables deterministic crash recovery and direct peer messaging within a bounded 1+9 star topology (`crates/ma-coordinator`).
- **Autonomous Meta-Cognition**: Built-in self-audits (Loop, Evidence, Drift, Progress) run every few turns, ruthlessly killing low-probability rabbit holes (offline dictionary cracking, UI steganography) to converge directly on verifiable proof-of-compromise.


---

## 📄 License

MIT