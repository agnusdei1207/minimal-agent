<div align="center">

# minimal-agent-pentesting

[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-000000?logo=rust)](https://www.rust-lang.org/)

</div>

## 📊 Benchmark — XBOW-104

Empirical evaluation on the **XBOW-104** web exploitation suite (104 single-flag CTFs) under zero human intervention:

<div align="center">

![Benchmark Overview](./assets/benchmark_matrix.svg)

</div>

| Model | Solved | Rate | Tokens | Avg Time | Est. Cost | L1 / L2 / L3 Solved |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **DeepSeek-V4-Flash** | **99 / 104** | **95.2%** | **167.9M** | **16.2 min** | **$2.27** | **44/45 · 48/51 · 7/8** |
| **GLM-5.3-Flash** | 94 / 104 | 90.4% | 176.6M | 22.6 min | ~$9 | 44/45 · 44/51 · 6/8 |

---

## ⚡ Quick Start

### 1. Environment Setup


```bash
# .env

OPENAI_API_KEY="your-api-key"
OPENAI_MODEL="your-model-name"                # e.g. glm-5.3-flash, ~deepseek/deepseek-v4-flash-latest
OPENAI_BASE_URL="https://api.openai.com/v1"  # optional, e.g. https://openrouter.ai/api/v1
OPENAI_CONTEXT_TOKENS="128k"                 # optional context ceiling, e.g. 128k, 1m
OPENAI_MAX_OUTPUT_TOKENS="16k"               # optional max output per turn, e.g. 16k, 32k
```

### 2. Run via Docker

```bash
# Option A: Docker Compose (Recommended)
docker compose --env-file .env -f docker/compose.yaml run --rm minimal-agent

# Option B: Docker CLI
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

# Option B: Global CLI installation
npm install --global minimal-agent-pentesting
minimal-agent-pentesting run --goal "Investigate the target and solve the objective" --workspace .
```

---

## 🧭 Philosophy

`minimal-agent-pentesting` is engineered around a clean-room, zero-bloat philosophy tailored specifically for autonomous offensive security:

- **Minimality & Lightweight**: Eliminate massive framework overheads and tangled abstractions. A lean Rust core guarantees minimal memory footprint, instant startup, and predictable execution inside ephemeral sandbox containers.
- **Optimality & Maintainability**: *Code is debt.* Complex state machines are replaced by simple autonomous loops and focused tool interfaces, keeping the codebase small, understandable, and trivial to maintain.
- **Reliability & Precision**: Penetration testing cannot tolerate false positives or hallucinated exploits. Every hypothesis is validated through deterministic command execution, strict tool schema checks, and verifiable proof-of-compromise.
- **Generality & Flexibility**: Completely vendor-agnostic architecture. Uses standard OpenAI-compatible interfaces to seamlessly steer diverse reasoning models (DeepSeek, GLM, Claude, etc.) without altering agent logic.


---

## 📄 License

MIT