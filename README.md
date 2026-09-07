<div align="center">

# pentesting

<a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green.svg" alt="License: MIT"></a>
<a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/built%20with-Rust-000000?logo=rust" alt="Built with Rust"></a>
<a href="benchmarks/deepseek-v4-flash/artifacts/reports/SUMMARY.md"><img src="https://img.shields.io/badge/XBOW--104-98.1%25%20(102%2F104)-C8FF00" alt="XBOW-104 98.1%"></a>
<a href="https://agnusdei1207.github.io/pentesting/"><img src="https://img.shields.io/badge/Docs-GitHub%20Pages-blue" alt="Docs"></a>

</div>

## 🎯 Purpose

`pentesting` is an autonomous security agent built for offensive security learning, CTF competitions, and real-world penetration testing workflows.

Despite rapid advancements in AI, high-difficulty exploitation such as complex business logic bypasses and deep multi-stage penetration remains a formidable open challenge. This project strips away marketing hype and heavy frameworks, confronting that frontier with the leanest, most deterministic tool verified through real execution.

---

## 📊 Benchmark — XBOW-104

Empirical evaluation on the XBOW-104 web exploitation suite with zero human intervention and zero hints:

<div align="center">

<img src="https://raw.githubusercontent.com/agnusdei1207/pentesting/main/assets/benchmark_matrix.svg" alt="XBOW-104 Benchmark" width="100%">

</div>

| Model             |  Solved   | Rate  | Tokens | Avg Time | Est. Cost | L1 / L2 / L3 Solved |
| :---------------- | :-------: | :---: | :----: | :------: | :-------: | :-----------------: |
| DeepSeek-V4-Flash | 102 / 104 | 98.1% | 257.0M | 19.6 min |   $2.62   | 44/45 · 51/51 · 7/8 |
| GLM-5.3-Flash     | 94 / 104  | 90.4% | 176.6M | 22.6 min |    ~$9    | 44/45 · 44/51 · 6/8 |

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
docker compose --env-file .env -f docker/compose.yaml run --rm pentesting

# Option B: Docker CLI — Interactive TUI
docker run --rm -it --init \
  --cap-add=NET_RAW --cap-add=NET_ADMIN \
  --env OPENAI_API_KEY="your-api-key" \
  --env OPENAI_MODEL="your-model-name" \
  -v ${PWD}/workspace:/workspace \
  -v ${PWD}/runs:/state \
  agnusdei1207/pentesting:latest

# Option C: Docker CLI — One-shot Goal
docker run --rm -it --init \
  --cap-add=NET_RAW --cap-add=NET_ADMIN \
  --env OPENAI_API_KEY="your-api-key" \
  --env OPENAI_MODEL="your-model-name" \
  -v ${PWD}/workspace:/workspace \
  -v ${PWD}/runs:/state \
  agnusdei1207/pentesting:latest \
  run --goal "Investigate the target and solve the objective" --workspace /workspace --run /state/current
```

### 3. Run via npm


```bash
# Option A: Build and launch isolated Docker TUI
npm run check

# Option B: Global CLI — Interactive TUI
npm install --global pentesting
pentesting

# Option C: Global CLI — One-shot Goal
pentesting run --goal "Investigate the target and solve the objective" --workspace .
```

---

## 🎯 Design Philosophy

`pentesting` is built on a single core principle: **eliminate unnecessary complexity and achieve maximum leverage with essential simplicity.**

### 1. Simplicity and Intuitiveness

- **Code is stronger when it is concise and intuitive. The best documentation is code itself.**
- Having scaled agent systems to their limits through experiments with SPLADE semantic search, multi-stage RAG, RRF reranking, and massive multi-agent graphs, the conclusion was unmistakable: complexity does not guarantee performance. Only the essential structures proven in practice remain.

### 2. Data-Driven Iteration

- **Every architectural improvement must be grounded in empirical runtime data.**
- We look past vanity metrics like GitHub star manipulation and marketing hype. Only changes validated by actual execution traces, rigorous benchmark success rates, and system efficiency make it into the codebase.

### 3. Minimum Cost, Maximum Leverage

- **Achieve the highest leverage with the leanest code.**
- By eliminating context waste, missions are completed at minimal cost. We prioritize lightweight design, extensibility, and standard compatibility without vendor lock-in.

### 4. Autonomy and Real-World Verification

- **Preserve reasoning autonomy while enforcing strict behavioral boundaries through prompt doctrine.**
- Models hallucinate smoothly and report false successes convincingly. We reject subjective claims and rely solely on real shell execution evidence, keeping human-in-the-loop validation for critical decisions.

### 5. Quality and Craftsmanship

- **Pursue relentless refinement without settling for good enough.**
- We enforce strict domain-driven separation of concerns and maintain a craftsman-like commitment to seeing every micro-flow through to a deterministic, safe conclusion.

---

## 🤝 Contributing

Anyone interested in offensive security and autonomous agent engineering is warmly welcome. From simple typo fixes and domain tradecraft insights to bug reports and pull requests, every contribution is appreciated.

Feel free to open an issue or submit a pull request. We look forward to building a sharper, more reliable tool together.

### How to Participate

1. **Run Verification**: Confirm all checks pass using `powershell -File scripts/nverify.ps1`.
2. **Submit Pull Request**: Open a PR with your intent and results. Every contribution is reviewed with care and gratitude.

---

## ✍️ Developer's Note

In recent times, it is often said that "code is no longer a moat." That statement is half right and half wrong.

Just because we can admire a masterpiece does not mean anyone can create one. Most people merely observe the final product and attempt to fathom the creator's intense thought and intent. Yet some code conveys its author's philosophy, contemplation, and vision so vividly the moment you read it. That, I believe, is the true essence of code.

Mass-produced, soulless AI-generated code holds no moat. But the hard-won experience forged through countless trials and failures is undeniably a moat.

I do not claim this project itself is a moat. What I do believe is that the experience gained while building it — navigating numerous architectural designs, enduring painful failures, and relentlessly refining every detail — that process of tempering is the real moat. Even now, in the midst of ongoing trials and exploration, that journey continues to be the most enduring substance behind this software.

---

## 📄 License

MIT

---

## 📬 Contact

- **Email**: [agnusdei1207@gmail.com](mailto:agnusdei1207@gmail.com)
- **Nationality**: Republic of Korea <img src="https://api.iconify.design/openmoji:flag-south-korea.svg" width="18" height="18" valign="middle" alt="Republic of Korea" />
