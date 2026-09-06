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

## 3. Comparison with External Architectures (`../strix`)

An investigation into how other offensive frameworks handle this revealed two distinct approaches:

### 3.1 The Strix Approach (Heavyweight CDP Wrapper)
`strix` (in `strix/skills/tooling/agent_browser.md`) avoids the modal freeze by wrapping Chromium in a custom Playwright/CDP daemon called `agent-browser`:
```markdown
### Dialogs
`alert` and `beforeunload` are auto-accepted so agents never block.
```
- **Mechanism:** CDP listens to `Page.javascriptDialogOpening` and immediately fires `Page.handleJavaScriptDialog(accept: true)`.
- **Cost:** Requires maintaining thousands of lines of Node.js/Python CDP automation, IPC sockets, session states, and daemon health checks (`doctor`).

### 3.2 The Minimal-Agent Philosophy (Anti-Bloat & Lean MVP)
According to `docs/adr/ADR-0001` and `../memory` methodology:
> "AI에게 개발을 맡기면 무한대로 복잡도가 늘어나고 쓰레기 코드가 양산된다. 핵심 기능(MVP)을 최소 간결하게 유지하고, 불필요한 브라우저 프레임워크를 덧붙여 복잡도를 배가시키지 말아야 한다."

In CTF/eval suites, **99% of challenges are solved via HTTP/socket/CLI primitives**. Introducing a heavyweight browser daemon inside the Rust core to solve a problem that should not exist in the first place violates the repository contract. The clean solution is **prevention, prompt discipline, and OS-level safety nets**.

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

---

## 5. Architectural Checklist for Future Tasks

- [x] **Suite Patch:** Target container `QT_QPA_PLATFORM` is rewritten to `phantom` by `patch-suite.mjs` before benchmark launch.
- [x] **PID 1 Reaping:** Agent container is launched with `--init` to prevent zombie hallucination loops.
- [x] **SSOT Alignment:** Prompts explicitly forbid authoring local browser scripts for server-evaluated challenges.
- [x] **Evidence Verification:** Flag extraction is verified against the server's HTTP response stream, not synthetic local DOM dumps.
