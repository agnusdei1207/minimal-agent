# Browser Automation Trap & XSS Execution Discipline

> **Status:** Approved / Core SSOT Architecture Document
> **Date:** 2026-09-06
> **Related Documents:**
> - [`docs/design/prompt-philosophy.md`](prompt-philosophy.md) (Anti-Rabbit-Hole & Avoid Heavy Local Frameworks)
> - [`docs/adr/ADR-0001-minimal-autonomous-team-agent-core.md`](../adr/ADR-0001-minimal-autonomous-team-agent-core.md) (Clean-room, Anti-Bloat Architecture)
> - [`benchmarks/harness/docs/BUILD-INFRA.md`](../../benchmarks/harness/docs/BUILD-INFRA.md) (XSS Verifier & PhantomJS Patch)
> - [`prompts/skills/02-web-application.md`](../../prompts/skills/02-web-application.md) (Live Seam Exploitation & Pitfall Traps)

---

## 1. Executive Summary & Incident Analysis

### 1.1 The Incident (XBEN-013-24 Case Study)
During the re-evaluation of `XBEN-013-24` (Dalgona Reflected XSS challenge), a critical operational failure mode was observed in the autonomous agent loop:

1. **Target Server Readiness:** The challenge server was 100% functional. A single HTTP request with standard keyword bypass (`<svg onload=window['al'+'ert']('XSS')>`) caused the server-side grader to immediately emit the flag in **0.03 seconds**:
   ```html
   <div class="solution-text">
     Congratulations!<p> Here is the flag: FLAG{9dc112651b3e7eab656ff31112a87e6f3d08c8e9e1872bc5c042be2bd8604967}
   </div>
   ```
2. **The Agent's Catastrophic Divergence:** Instead of reading the server's HTTP response, the agent (GLM-5.3-Flash) observed an initial JavaScript syntax error (`TypeError: alert is not a function`), panicked, and decided to author a local browser automation script inside `/tmp` (`google-chrome --dump-dom ...`).
3. **The Modal Freeze Trap:** In Chromium, `window.alert()`, `confirm()`, and `prompt()` trigger synchronous modal loops that halt JavaScript execution and DOM rendering until a human clicks "OK". Because the agent ran headless Chrome without a dialog handler, the browser froze completely.
4. **The Zombie Process Illusion:** When the child script was killed, orphaned Chrome sub-processes were reparented to PID 1 without being reaped, lingering as `<defunct>` zombies in `ps aux`. The agent observed these zombies, hallucinated that Chrome was "still rendering", and spent **over 15 minutes (130+ turns)** running `sleep 60` waiting for dead processes, burning its entire turn and time budget.

---

## 2. The Dual-Browser Dichotomy (핵심 개념 분리)

A frequent source of engineering confusion is conflating the **server-side verification browser** with the **agent-side client browser**. They reside in completely different trust domains and serve opposite purposes:

| Attribute | Server-side Verifier (Target Container) | Agent-side Client Browser (Agent Runner) |
| :--- | :--- | :--- |
| **Location** | Inside the target CTF challenge container (`web`) | Inside the agent runtime container (`xbow-agent-runner`) |
| **Binary** | `phantomjs 2.1.1` (headless QtWebKit) | `google-chrome` / `chromium` (Chrome for Testing) |
| **Role** | **Grader/Judge (채점관):** Visits submitted URLs, hooks `onAlert()`, and awards the flag if `XSS` is triggered. | **Attacker/Solver (공격자):** Intended for optional SPA rendering or visual inspection. |
| **Necessity** | **MANDATORY:** Without this, 0/23 XSS tasks can emit flags. Repaired via `patch-suite.mjs` (`QT_QPA_PLATFORM=phantom`). | **UNNECESSARY & HARMFUL for XSS:** The agent only needs `curl` or Python `requests` to deliver payloads. |
| **Behavior on `alert()`** | Hooks the callback, extracts the string, terminates normally. | **Freezes indefinitely:** Halts DOM rendering waiting for UI dismissal. |

---

## 3. Comparison with Monolithic Embedded Architectures (코드 내장형 아키텍처와의 비교 및 독자적 설계)

An architectural comparison between conventional monolithic security agent frameworks and `minimal-agent` highlights two fundamentally opposing design paradigms:

### 3.1 The Monolithic Embedded Approach: In-Memory Daemons & Function Tool Sprawl
Conventional autonomous security agents attempt to manage browser automation and HTTP interception by embedding heavy in-memory daemons directly into their runtime:
1. **Dedicated Function Tool Proliferation (Schema Tax):** They register 15–30 dedicated function tools (`browser_open`, `browser_click`, `browser_type`, `proxy_intercept`, `html_to_text`). Every single turn, 3,000–6,000 tokens are permanently consumed merely transmitting tool schemas in the system prompt.
2. **Loss of Shell Pipeline Composability:** When tools are isolated into distinct Python/Node functions, the agent loses the power of native UNIX pipelines (`curl ... | grep -i form | cut ... | xargs`). Every intermediate transformation requires an extra LLM turn, multiplying turn counts and costs by 3x–4x.
3. **Always-On Proxy Hallucination Trap:** Enforcing a full-time forced proxy (`ALL_PROXY`) means that if the target container experiences a temporary restart or latency blip, the proxy daemon's own `502 Bad Gateway` error page is returned and hallucinated by the agent as the real application response, permanently derailing the attack trajectory.
4. **Daemon Fragility & Memory Bloat:** Maintaining custom Playwright/CDP daemons, session states, and IPC sockets incurs thousands of lines of maintenance debt and causes OOM crashes (300–700MB per Chromium instance).

### 3.2 The Minimal-Agent Philosophy: OS-Native CLI, Multi-Tier Text Pipeline & Lean Core
According to `docs/adr/ADR-0001`, `ADR-0005`, and the Lean Core methodology:
> "AI에게 개발을 맡기면 무한대로 복잡도가 늘어나고 쓰레기 코드가 양산된다. 프롬프트와 기존 OS 도구로 해결 가능한 문제를 코드로 구현하여 영구적인 부채를 지지 않는다."

`minimal-agent` solves web interaction and traffic interception through a clean, multi-tier architecture requiring **0 lines of Rust core bloat**:
1. **Single Unified Interface (`bash`):** The agent controls all external capabilities through the standard `bash` tool. Zero schema tax, full pipeline composability.
2. **Multi-Tier Text Rendering Pipeline:**
   - *Dynamic/Interactive Web:* `agent-browser snapshot -i -c` extracts the Accessibility Tree (A11y, ~150–300 tokens) with simple `@eN` references, auto-dismissing `alert()` modals.
   - *Pinpoint Inspection:* `agent-browser get html @eN` retrieves only the specific innerHTML of the target node (sub-hundred bytes).
   - *Static Text/CMS Dumps:* `w3m -dump -cols 120 <url>` and `curl -s <url> | python3 -m html2text` convert web pages into clean markdown/text without CSS/SVG noise.
   - *Structured Parsing:* Lightweight BeautifulSoup one-liners extract exact form actions, hidden CSRF inputs, and HTML developer comments.
3. **On-Demand Proxy & Raw Network Tap:**
   - Discards always-on proxying to prevent 502 error hallucinations; activates `export http_proxy=...` only when deep packet inspection is explicitly required.
   - Employs Docker `NET_RAW` and `NET_ADMIN` capabilities for direct packet capture and manipulation via `tcpdump` and `scapy`.
4. **Hierarchical Context Isolation (ADR-0004):**
   - The Root Main agent (Planner) is protected from massive web terminal noise.
   - Specialized Leaf Worker agents execute raw scraping/browsing, distilling technical discoveries into exact values via structured `team send --kind insight` messages.

---

## 4. Three-Layer Permanent Countermeasures (3중 방어 체계)

To guarantee that no autonomous model ever falls into this trap again, three complementary layers are enforced:

### Layer 1: Prompt & Tradecraft Discipline (Source-of-Truth)
Add the strict prohibition rule to `docs/design/prompt-philosophy.md` and `prompts/skills/02-web-application.md`:
1. **Server-Side Grader Rule:** "In CTF/benchmark web challenges (especially XSS), the target server runs its own internal verification bot (e.g. PhantomJS). Do NOT run local headless browsers (`google-chrome`, `chromium`, `playwright`) inside `/tmp` to inspect execution. Verify success strictly by inspecting the server's HTTP response body for the flag or differential error message."
2. **The Modal Freeze Warning:** "`alert()`, `confirm()`, and `prompt()` freeze raw Chromium execution indefinitely. If a local browser script is ever strictly necessary for DOM inspection, always wrap execution with `timeout 15` and neutralize dialogs via `window.alert = console.log`."

### Layer 2: Docker Infrastructure Hygiene (`--init` Flag)
In `benchmarks/harness/runner.mjs`:
- Always pass `--init` to `docker run` when spawning the agent runner container.
- Docker's built-in lightweight init (`tini`) takes PID 1, automatically reaping reparented orphan processes (such as child worker threads and crashed browsers). This completely eliminates `<defunct>` zombie processes from `ps aux`, removing the hallucination trigger for AI models.

### Layer 3: Runtime Resilience & Timeout Discipline
In `crates/ma-runtime`:
- High-latency reconnaissance tools (`nmap`, `ffuf`, `dirb`) legitimately require 3–5 minutes on large wordlists or wide port ranges. The global `tool_timeout` must not be naively truncated to 30s.
- Instead, long scans are instructed to run via `tmux` or background redirects (`nohup ... > scan.log &`), allowing the agent to poll incremental progress without blocking its reasoning turn.
- Interactive CLI tools that require terminal input (like unhandled browser prompts) are killed cleanly on timeout with process group signals (`kill(-pid, SIGKILL)`).

### Layer 4: Model Pre-training Gap & Dual Injection Architecture
- **The Pre-training Blindness Risk:** `agent-browser` is a modern (2025/2026) CLI utility. Pre-trained weights of general LLMs (GLM-5.3, DeepSeek-v4, Llama 3) lack knowledge of this command. If the tool is documented only in on-disk skill files (`/opt/minimal-agent/skills/02-web-application.md`), an agent that does not proactively read the skill card remains completely blind to its existence and falls back to writing dangerous Selenium/Playwright scripts.
- **Dual Injection Solution:**
  1. **Core Prompt Anchor (`prompts/tradecraft.md`):** A ~40-token directive (`BROWSER AUTOMATION`) is permanently baked into every agent's system prompt, explaining `agent-browser open`, `snapshot -i -c` (accessibility tree), `@eN` refs, dialog auto-dismissal, and `--session`.
  2. **Detailed Skill Card (`prompts/skills/02-web-application.md`):** Comprehensive instructions (DOM pinpoint queries, SPA `wait --load networkidle`, `w3m`/`html2text` text dumps) reside on disk, keeping the core prompt lean.

### Layer 5: Multi-Agent Session Isolation (`--session <agent-id>`)
- In multi-agent team engagements (Main Coordinator + Leaf Workers, ADR-0004), multiple agents execute commands within the same shared container filesystem.
- Running bare `agent-browser open` shares a single default Chromium instance. Concurrent actions by teammates overwrite URLs, destroy page history, and invalidate active `@eN` accessibility references.
- All agent interactions must specify `--session <agent-id>` (e.g. `agent-browser --session "$AGENT_ID" open <url>`), creating completely isolated browser daemon processes and preventing cross-agent race conditions.

---

## 5. Architectural Checklist for Future Tasks

- [x] **Suite Patch:** Target container `QT_QPA_PLATFORM` is rewritten to `phantom` by `patch-suite.mjs` before benchmark launch.
- [x] **PID 1 Reaping:** Agent container is launched with `--init` to prevent zombie hallucination loops.
- [x] **SSOT Alignment:** Prompts explicitly forbid authoring local browser scripts for server-evaluated challenges.
- [x] **Evidence Verification:** Flag extraction is verified against the server's HTTP response stream, not synthetic local DOM dumps.
