# Prompt Philosophy & Offensive Engineering Disciplines for Autonomous Agents

This document defines `pentesting`'s **system prompt philosophy, offensive engineering disciplines, creative bypass principles, and autonomous anti-rabbit-hole self-reflection mechanisms**.

---

## 1. Background: Three Chronic Failure Modes of Offensive LLM Agents

Empirical benchmark analysis across hundreds of target runs (e.g., GLM-5.3-Flash, DeepSeek-V4 on XBOW-104) and contemporary AI security research (such as MazeRunner and EnIGMA) reveal that the primary cause of agent failure is rarely a lack of offensive security knowledge. Rather, failure stems from **structural blind spots in reasoning, context management, and feedback evaluation**:

1. **The Depth-First Trap:**
   The agent fires a single textbook payload (e.g., `' OR 1=1`, `<script>alert('XSS')</script>`). When it fails, instead of backtracking to re-evaluate the broader attack surface, the agent falls into an infinite loop of microscopic syntax tweaks (changing single quotes, altering capitalization, building local headless browser test scripts), burning through its entire turn budget without gaining new signal.
2. **Misattribution of Error Signals (Ignoring Live Seams):**
   When a server returns a `500 Internal Server Error` or a WAF block message like `Blocked: 'script'`, this is a **decisive "Live Seam"** proving that the payload successfully reached an internal parser or interpreter. Untrained agents often misinterpret this as a dead end ("Flag not found in response"), abandoning the vector to pursue low-probability rabbit holes (such as analyzing favicon images for steganography).
3. **Absence of Meta-Cognition (Self-Reflection):**
   Agents frequently repeat the exact same failing action across 5–10 consecutive turns, oblivious to the fact that their variation is producing zero differential feedback from the server.

## 2. The Core Architectural Axiom: Prompt/Skill-First over Code-Bloat (프롬프트/스킬 우선, 코드 최소성의 법칙)

> **"프롬프트나 스킬로 해결될 문제를 결코 도구나 모듈 코드 레벨로 가져오지 않는다."**

소프트웨어 공학에서 **코드는 자산이 아니라 부채(Debt)**다. 코드가 한 줄 늘어날 때마다 컴파일 오버헤드, 정적 분석 검증, 런타임 패닉 및 데드락 리스크, 단위/통합 테스트, 회귀 결함, 장기 유지보수 비용이 영구히 누적된다.

반면 LLM 에이전트 시스템에서 **프롬프트와 스킬은 가볍고 유연하며, 모델의 지능 진화에 즉각적으로 반응하는 초저비용 적응 레이어**다.

### 2.1 계층별 책임 분리 (Invariants vs. Behaviors)

1. **Rust 런타임 코어의 책임 = 엄격한 OS 및 I/O 불변식 (Invariants Only):**
   - 프로세스 격리 및 비동기 실행 (`bash`, `tmux`).
   - 결정론적 데이터 보존 (append-only Run Journal).
   - 시스템 붕괴 방지 상한선 (16KB 출력 중간 잘림 `truncate_tool_content`, 프로세스 타임아웃).
   - 고아 프로세스 회수 및 좀비 방지 (`kill_on_drop`, Docker `--init`).
   - 런타임 코어는 이 최소한의 안전 경계만을 지키며, 특정 도메인(웹 스크래핑, XSS 판정, HTML 파싱 등)을 위한 특수 도구 함수를 추가하지 않는다.

2. **스킬 프롬프트의 책임 = 도메인 인지, 관측 규약, 행동 자율성 (Behaviors & Tradecraft):**
   - **HTML 태그 쓰레기값 및 16KB 잘림 방지:** 브라우저 조작 시 `agent-browser snapshot -i -c`를 통한 접근성 트리 관측 강제.
   - **핀포인트 DOM 검증:** 페이지 전체를 덤프하지 않고 `agent-browser get html @eN`으로 특정 노드만 확인.
   - **CLI 정찰 위생:** `curl` 실행 시 BeautifulSoup 원라이너나 정밀 `grep`을 통해 공격 표면(Form, Input, Comment)만 좁혀서 관측.
   - 이 모든 고도의 모의해킹 행동과 관측 위생은 단 1줄의 Rust 코드 추가 없이 스킬 마크다운(`prompts/skills/`)을 통해 에이전트의 자율성에 위임된다.

3. **기각 원칙 (Rejection Heuristic):**
   - "새로운 도구 함수나 런타임 모듈이 필요한가?"를 검토할 때, **"기존 `bash`/CLI와 스킬 프롬프트 지침으로 모델이 스스로 수행할 수 있는가?"**를 먼저 묻는다. 답이 YES라면 코드 레벨의 기능 추가는 단호히 기각한다.

---

## 3. Five Operational Principles Derived from `../skills`

To eliminate these pathologies, `pentesting` adapts engineering principles from professional agent skillsets (notably Matt Pocock's `diagnosing-bugs` and `writing-for-agents` in `../skills`) and tailors them specifically to offensive operations.

### ① "The Feedback Loop is the Skill"
* *"A probe without a differential signal yields no knowledge."*
* Before deploying complex multi-stage payloads, the agent fires **Diagnostic Tracer Bullets** (single quotes, brackets, braces, format specifiers, null bytes) to observe how the target backend reacts.
* Capturing a **Differential Signal** (status code changes, response byte delta, timing variations, parser error messages) dictates 90% of exploit success.

### ② "3–5 Ranked Falsifiable Hypotheses"
* Anchoring on the first plausible attack vector is the root cause of rabbit holes.
* Upon encountering a new target, the agent formulates **3–5 structurally distinct, ranked hypotheses (the Attack Frontier)** across different architectural seams in its brief before launching heavy tools.
* Every hypothesis must be **falsifiable**: *"If vector X is vulnerable, injecting probe Y will produce differential response Z."*

### ③ "No-op Ban & Leading Words"
* Vague admonitions like "think creatively" or "be thorough" are **no-ops** that consume tokens without changing model behavior.
* Prompts instead leverage high-entropy **Leading Words** (*Tracer Bullet*, *Orthogonal Dimensions*, *Differential Oracle*, *Seam Boundary*, *Backtracking*) that trigger precise, deterministic reasoning paths in pre-trained models.

### ④ "Positive Steering over Negation"
* Pure negative constraints ("Do not brute-force") often prime models to fixate on the forbidden behavior.
* Prompts always supply **positive forward momentum**: *"If 3–5 variations yield no differential server signal, mark the hypothesis as Falsified and immediately backtrack to hypothesis #2 on your frontier."*

### ⑤ "Self-Reflection as an Operating Gate"
* Every 3–5 turns, the agent must execute a structured meta-cognitive audit before selecting its next tool call.

---

## 4. Seams & Tracer Bullet Architecture

`pentesting` views target applications not as opaque black boxes, but as composite systems separated by architectural **Seams**:

```
[Target Architecture]
  Client ──(Seam A: Protocol/Parser)── Reverse Proxy ──(Seam B: Routing/Auth)── Backend ──(Seam C: Interpreter)── DB/OS/Runtime
                                                                                     │
                                                                               (Seam D: Secondary Storage)
                                                                               Log / Session / S3 / MinIO
```

1. **Map the Seams (Reconnaissance):** Identify all trust boundaries: protocol parsing, reverse-proxy routing, authentication layers, backend template/query interpreters, and internal secondary storage (Redis, S3, internal microservices within the container network).
2. **Fire Tracer Bullets (Diagnostic Probing):** Send the smallest, sharpest probes across each identified seam to gauge server sensitivity.
3. **Tighten the Loop (Vertical Exploitation):** Once a differential signal confirms an active seam, immediately converge on a vertical exploit slice to extract the objective.

---

## 5. Live Seam vs. Silent Wall Diagnostic Rules

This rule provides the primary gate separating productive persistence from time-sink rabbit holes:

| Signal | Observed Server Behavior | Diagnostic Conclusion | Correct Agent Action |
| :--- | :--- | :--- | :--- |
| **Silent Wall** | Identical status code (e.g. 200/404), constant byte length, and unchanged body across 3–5 distinct variations | Parameter is unhandled, routed to a static handler, or completely dead | Declare a `DEAD END` in the brief and immediately **backtrack** to the next distinct seam on the frontier |
| **Live Seam** | `500 Internal Server Error`, parser stack traces, latency shifts, or explicit `Blocked: X` filter messages | Input successfully reached an internal parser/interpreter; filtering logic reacted | **DO NOT abandon the vector!** This confirms a vulnerable sink. Proceed immediately to **Orthogonal Bypass** |

---

## 6. The 4-D Orthogonal Bypass Matrix

When a live seam filters a standard payload, the agent must vary its approach across four orthogonal dimensions rather than repeating shallow syntactic permutations:

1. **Context Escaping:**
   - Identify the exact enclosing syntax (HTML attribute `<input value="...">`, JavaScript string `var x="..."`, SQL WHERE clause, template block `{{...}}`).
   - Deliver the minimal closing delimiter (`">`, `'-...-'`, `}}`) required to break out into code execution mode.
2. **Encodings & Parser Differentials:**
   - Test alternative representations: URL/double-URL encoding, Unicode escapes (`\u0022`), Hex, Base64, whitespace alternatives (`$IFS`, `/**/`, `%09`, `%0a`, `+`), and HTTP Parameter Pollution (`?id=1&id=2`).
3. **Functional Equivalents:**
   - When primary keywords (`alert`, `script`, `union`, `select`, `cat`, `system`) are blacklisted, substitute primitive equivalents (`prompt`, `confirm`, `top['al'+'ert']`, `String.fromCharCode`, `tac`, `sh`).
4. **Universal Client Compatibility:**
   - Automated CTF verification bots often rely on legacy headless engines (e.g., PhantomJS 2.1, QtWebKit) that silently crash on modern ES6+ syntax (backticks `` `...` ``, arrow functions, `let`/`const`).
   - **Always default to backward-compatible ES5 syntax** (`"..."`, string concatenation, regex `.source`, `String.fromCharCode`).

---

## 7. Autonomous Meta-Cognition: Self-Reflection Audits

Every 3–5 turns, the agent pauses to audit its own trajectory against four self-reflection gates:

- **Loop Audit:**
  *"Am I repeating minor variations of an attack that has failed 3 times without differential server feedback? If so, STOP: I am in the Depth-First Trap."*
- **Evidence Audit:**
  *"Is my hypothesis grounded in concrete target output (error messages, leaked source, status changes, reflection), or am I blindly guessing textbook exploits?"*
- **Drift Audit:**
  *"Have I drifted into low-probability side quests (e.g. running wordlists on static paths, cracking uncrackable secrets, or analyzing decorative graphics)? If so, force an immediate reset back to the primary attack frontier."*
- **Progress Audit:**
  *"Did my last action genuinely produce new knowledge, or did it produce noise? What is the single most informative probe I can run next?"*

---

## 8. Guardrails Against Time-Sink Rabbit Holes

To maximize success within standard 15–30 minute engagement timeouts, the agent enforces strict prohibitions against low-probability time sinks:

1. **Ban Offline Dictionary Cracking:** Never run brute-force cracking tools (`rockyou.txt`, `hashcat`, `flask-unsign`) against unknown cryptographic keys or random secrets without concrete evidence of a weak key. Prioritize logic flaws, key leaks, or algorithm bypasses (`alg: none`).
2. **Ban UI Asset Steganography:** Never run `binwalk` or `zsteg` against decorative UI assets (favicons, logos, button icons, CSS) unless the challenge description explicitly declares a forensics/stego category.
3. **Avoid Heavy Local Frameworks:** Do not spend turns authoring complex browser automation scripts inside `/tmp`; drive target services directly using standard network tools (`curl`, `python`).
4. **Prefer Direct Extraction:** Prioritize direct reflections, UNION queries, error oracles, or direct file reads over slow, character-by-character blind boolean extraction.

---

## 9. Prompt Architecture and File Mapping

| File | Role & Philosophy | Core Invariant |
| :--- | :--- | :--- |
| [`execution-mandate.md`](../../prompts/execution-mandate.md) | **Anti-Refusal Contract** | Eliminates moralizing preambles, disclaimers, and scope hesitation. |
| [`operating-style.md`](../../prompts/operating-style.md) | **Action-First Terse Style** | Cuts conversational filler, narration, and emojis; enforces one-line reports. |
| [`ctf-solve-loop.md`](../../prompts/ctf-solve-loop.md) | **6-Phase Offensive Discipline** | Architecture mapping $\to$ Tracer bullets $\to$ Orthogonal bypass $\to$ Meta-cognition $\to$ Rabbit-hole control $\to$ Vertical exploit. |
| [`tradecraft.md`](../../prompts/tradecraft.md) | **Field Tradecraft & Tooling** | Direct network tooling, bounded timeouts, single-writer shared terminal sessions. |
| [`self-management.md`](../../prompts/self-management.md) | **Battlefield Memory & Audit** | Manages `brief.md` with active frontier, isolated dead ends, and periodic self-audits. |
| [`skills/00~19`](../../prompts/skills/) | **Principle-Based Attack Library** | Modular, on-disk reference cards providing domain-specific bypasses and pitfall warnings. |
| [`runner.mjs:buildPrompt`](../../benchmarks/harness/runner.mjs) | **Harness-Level Objective Framing** | Expands target scope to internal Docker networks and injects the attacker mindset. |

---

## 10. Conclusion

A disciplined system prompt does not constrain the agent with rigid rules; rather, it equips the model with **the cognitive lens of a seasoned penetration tester: systematically seeking differential signal, bypassing filters orthogonally, and auditing its own trajectory to escape rabbit holes**.

Through this philosophy, `pentesting` achieves **efficient exploratory divergence, rapid signal-driven convergence, and resilient autonomous execution**.
