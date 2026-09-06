# Hierarchical Tree Orchestration Design Exploration Notes

This document explores the architectural possibilities, trade-offs, and failure modes of moving beyond `minimal-agent`'s original **star-topology single-level team (1 Main + ≤9 Workers, Depth 1)** toward **recursive trees of bounded depth with strict separation between supervisor (internal) nodes and worker (leaf) nodes**.

---

## 1. The Proposed Architecture Model: "Supervisor-Worker Pure Hierarchy"

### 1.1 Core Principles
1. **Strict Role Separation (Internal vs. Leaf):**
   - **Internal / Supervisor Nodes:** Agents that have children. They **never invoke environment tools (`shell`, `workspace`) directly**. Their sole responsibility is to partition and scatter tasks to child nodes, synthesize and reduce their progress/insights, and report concise status to their parent node.
   - **Leaf / Worker Nodes:** Execution agents that have no children. They are the **sole entities interacting with the external environment** by running tools. They report technical evidence directly to their supervisor. They cannot spawn subagents.
2. **Unidirectional Vertical Communication & Synthesis:**
   - Instructions flow downward; findings, exact values, and synthesis bubble upward.

```text
                  [ Root / Main ] (Depth 0: Overall Strategy & Final Reporting)
                   /             \
       [ Track Lead A ]        [ Track Lead B ] (Depth 1: Task Partitioning & Synthesis)
        /            \              |
  [ Leaf W1 ]    [ Leaf W2 ]    [ Leaf W3 ] (Depth 2: Tool Execution)
  (tool: shell)  (tool: shell)  (tool: shell)
```

---

## 2. Algorithmic Classification & Properties

| Dimension | Star Topology (Original minimal-agent) | Pure Hierarchical Tree (Exploratory Model) |
| :--- | :--- | :--- |
| **Graph Topology** | $K_{1, n}$ (Depth 1, radial star graph) | $N$-ary Tree (Hierarchical tree structure) |
| **Algorithmic Pattern** | Single-Level MapReduce / Scatter-Gather | Multi-Stage Divide-and-Conquer / Actor Supervision Tree |
| **Tool Execution Rights** | Main (can execute) + Workers (execute) | **Strictly Leaf nodes only** |
| **Intermediate Role** | None (Single lead directly orchestrates all) | Partitioning + Semantic Reduction + Relay |
| **Bounds** | Active Team $\le 10$, Depth $= 1$ fixed | Bounded Depth $D \le 3$, Subtree and Team Caps |

---

## 3. Expected Advantages (Pros)

1. **Role Clarity and Context Purity:**
   - Supervisory nodes are never polluted by voluminous raw tool outputs (e.g., massive scan dumps or build logs). They maintain clean strategic context in their `brief.md`, dramatically reducing hallucinations and goal drift.
2. **Scalability for Large Complex Projects:**
   - Overcomes the single coordinator's cognitive span-of-control limit (typically 5–10 concurrent agents), allowing subproject leads to independently manage specialized tracks (e.g., web frontend vs. backend auth vs. cloud infrastructure).
3. **Natural Hierarchical Semantic Compaction:**
   - Raw 1,000-line tool logs at the leaf tier compress into 3-line actionable insights at the lead tier, which distill into a 1-line tactical update at the root.

---

## 4. Critical Risks and Trade-offs (Cons & Challenges)

1. **Exponential Latency and Turn Explosion:**
   - For a leaf discovery to reach the root requires at least $D$ sequential LLM turns (Leaf $\to$ Lead $\to$ Root) in serial.
   - Even a simple 1-line verification task (e.g., "Check port 80 banner") consumes 4–6 LLM turns across the tree before the root can act on it.
2. **Information Degradation (The "Telephone Game"):**
   - In offensive security and CTF tasks, **exact distinguishing values** (e.g., memory leak offsets, cryptographic nonces, specific HTTP query strings) are critical. Multi-hop summarization risks abstracting these details away (e.g., "an offset was discovered"), paralyzing decision-making at the root.
3. **Spawn Storms and Infinite Evasion:**
   - When faced with an ambiguous or challenging problem, LLMs tend to procrastinate by recursively spawning subagents rather than solving the issue, causing catastrophic token and budget exhaustion.
4. **Crash Recovery and Journal Replay Complexity:**
   - When an intermediate node crashes or encounters a provider fault, garbage-collecting its entire subtree, recovering active permits, and deterministically replaying the journal becomes orders of magnitude more complex.

---

## 5. Practical CTF/Pentest Evaluation & Final Architectural Decision

- **CTF Operational Reality:** Most CTF and target-oriented penetration tests require **horizontal concurrency** (e.g., probing 5 distinct ports or testing 3 orthogonal vulnerabilities concurrently) rather than deep recursive delegation. However, complex multi-stage objectives (e.g., internal network pivoting) benefit substantially from bounded sub-leads.
- **Adopted Architecture (ADR-0004):**
  - **Bounded 3-Depth Cap:** The tree depth is strictly clamped to a maximum of 3 (`Root (0) -> Lead (1) -> Leaf (2)`), permanently preventing spawn storms.
  - **Exact Value Preservation Protocol:** System prompts strictly mandate that intermediate leads preserve raw technical values (endpoints, credentials, payloads) verbatim when relaying insights upward.
  - **Restricted Messaging Topology:** Inter-agent communication is bounded to direct parents, direct children, and same-parent siblings, keeping message bus complexity deterministic.
  - *Full normative specifications are codified in [ADR-0004](../adr/ADR-0004-bounded-three-depth-hierarchical-orchestration.md).*
