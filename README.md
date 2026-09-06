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

---

## 🧭 Architecture & Offensive Philosophy

`minimal-agent-pentesting` is engineered around a clean-room, zero-bloat philosophy formalized in [`docs/design/`](docs/design/) and proven empirically on real-world benchmark targets.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          LLM Reasoning & Intelligence                       │
│    (DeepSeek-V4-Flash / GLM-5.3-Flash / Claude via OpenAI-Compatible API)   │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
      ┌────────────────────────────────┼────────────────────────────────┐
      ▼                                ▼                                ▼
┌─────────────────────────┐  ┌─────────────────────┐  ┌───────────────────────┐
│ Prompts & Skills Layer  │  │   Coordination      │  │   Context Engine      │
│ (prompts/skills/00~19)  │  │ (crates/ma-coord)   │  │ (crates/ma-context)   │
│ • Tracer Bullet Probing │  │ • Bounded Star      │  │ • Owned brief.md      │
│ • 4-D Orthogonal Bypass │  │   Topology (1+9)    │  │ • 80% Threshold Local │
│ • Anti-Rabbit-Hole Gate │  │ • Direct Messaging  │  │   Semantic Compaction │
└─────────────┬───────────┘  └──────────┬──────────┘  └───────────┬───────────┘
              │                         │                         │
              └─────────────────────────┼─────────────────────────┘
                                        ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                   Lean Rust Core Runtime Invariants                         │
│  crates/ma-runtime · crates/ma-journal · crates/ma-core · crates/ma-tui     │
│  • Single Append-Only Durable Event Journal (Zero external DB / Zero RAG)   │
│  • Deterministic Process Sandboxing (Docker --init, bash/tmux, kill_on_drop)│
│  • Hard Boundary Guards (16KB tool truncation, execution timeouts)          │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1. The Core Architectural Axiom: Prompt/Skill-First over Code-Bloat
> *"Never solve in Rust runtime code what can be solved in a prompt or on-disk skill."*

In software engineering, **code is debt**. Every additional line of runtime code brings compile overhead, panic/deadlock risks, regression vulnerabilities, and maintenance drag. Conversely, system prompts and markdown skill cards are lightweight, highly expressive, and immediately adapt as underlying LLM intelligence evolves.

- **Rust Runtime (`crates/ma-runtime`, `crates/ma-core`)**: Enforces strict OS and I/O **invariants only**—process sandboxing, deterministic append-only state journals, execution timeouts, orphan process cleanup (`kill_on_drop`), and safety limits (16KB output truncation via `truncate_tool_content`). The core runtime contains zero target-specific heuristics or scrapers.
- **Offensive Skills Layer (`prompts/skills/`)**: Dictates tactical tradecraft and observation hygiene without adding a single line of Rust code—accessibility tree snapshots (`agent-browser snapshot -i -c`) to prevent DOM bloat, surgical one-liner reconnaissance via `python`/`BeautifulSoup`, and domain-specific attack heuristics.

### 2. Architectural Seams & Tracer Bullet Methodology
Rather than treating targets as monolithic black boxes, `minimal-agent` decomposes web targets into architectural **Seams** (Protocol Parsing, Reverse-Proxy Routing, Authentication Layers, Template/Query Interpreters, and Internal Container Networks):

```
Target Seams: Client ──(Protocol)── Proxy ──(Routing/Auth)── Backend ──(Interpreter)── DB/OS/Storage
```

1. **Map the Seams**: Identify trust boundaries and parser handoffs.
2. **Fire Diagnostic Tracer Bullets**: Instead of heavy multi-stage payloads, dispatch minimal, sharp syntax probes (quotes, brackets, format specifiers, null bytes) to measure backend sensitivity.
3. **Capture Differential Signals**: Exploit progress is governed by differential server feedback (status code changes, response byte delta, timing discrepancies, parser stack traces).
4. **Live Seam vs. Silent Wall**:
   - **Live Seam**: A `500 Internal Server Error`, stack trace, or WAF alert (`Blocked: 'script'`) proves the probe penetrated internal parsers. **Never abandon a live seam**—converge immediately on orthogonal bypasses.
   - **Silent Wall**: Identical response length and body across 3–5 variations indicates an unhandled parameter or static route. Declare a dead end and **backtrack** to the next hypothesis on the attack frontier.

### 3. The 4-D Orthogonal Bypass Matrix
When a live seam blocks a standard payload, the agent strictly avoids the **Depth-First Trap** (shallow repetitive syntax tweaks). Instead, it traverses four orthogonal dimensions simultaneously:

- **Context Escaping**: Pinpoint the precise enclosing syntax (`<input value="...">`, JS string literal, SQL condition) and craft minimal closing delimiters (`">`, `'-...-'`, `}}`).
- **Encodings & Parser Differentials**: Leverage double-URL encoding, Unicode escapes (`\u0022`), whitespace alternatives (`$IFS`, `/**/`, `%09`, `%0a`), and parameter pollution.
- **Functional Equivalents**: Substitute blacklisted keywords with primitive alternates (`prompt`, `top['al'+'ert']`, `String.fromCharCode`, `tac`, `sh`).
- **Universal Client Compatibility**: Target automated verification bots using backward-compatible ES5 JavaScript, preventing headless browser crashes caused by modern ES6+ template literals or arrow functions.

### 4. Single Append-Only Journal & Bounded Team Orchestration
Autonomous multi-agent collaboration often suffers from context fragmentation and runaway recursion:

- **Zero External DB / No RAG Overhead**: Communication, state management, and crash recovery are all derived from a single durable, append-only run journal ([`crates/ma-journal`](crates/ma-journal)). State is projected in-memory via deterministic event folding.
- **Bounded Star Topology**: Teams are strictly capped at 1 Coordinator (`main`) and up to 9 specialized workers ([`crates/ma-coordinator`](crates/ma-coordinator)), eliminating infinite subagent spawn storms.
- **Isolated Memory (`brief.md`) & Local Compaction**: Each worker independently maintains its tactical brief. When an individual context window reaches ~80%, only that worker undergoes semantic compaction ([`crates/ma-context`](crates/ma-context)), preserving global coordinator context purity.

### 5. Meta-Cognitive Audits & Anti-Rabbit-Hole Guardrails
Every 3–5 turns, the agent pauses tool execution to pass through four self-reflection gates ([`docs/design/prompt-philosophy.md`](docs/design/prompt-philosophy.md)):
- **Loop Audit**: *"Am I repeating micro-variations of a failed attack without differential feedback? If so, halt and backtrack."*
- **Evidence Audit**: *"Is this hypothesis grounded in observed target output, or blind textbook speculation?"*
- **Drift Audit**: *"Have I wandered into low-probability side quests (offline dictionary cracking, static path wordlists, decorative UI steganography)? Force an immediate reset."*
- **Progress Audit**: *"Did the previous action yield new knowledge? What is the single most informative probe to fire next?"*


---

## 📄 License

MIT