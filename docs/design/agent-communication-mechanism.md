# Inter-Agent Communication Mechanism (Messages Array Perspective)

This document explains the principles of inter-agent communication in `minimal-agent` from the perspective of the **underlying conversation array (`messages: [...]`) exchanged during LLM API calls**.

---

## 1. The Underlying Reality of Communication

At its core, an LLM API invocation is simply transmitting the **entire historical conversation array (`messages`)** to the model.

Therefore, in `minimal-agent`, **"inter-agent communication"** functions through the following array manipulation:

> **The central runtime intercepts a message emitted via a tool call by sender (A) and appends it as a new `user` message to the tail of receiver (B)'s conversation array.**

```text
[Agent A's Conversation Array]                 [Agent B's Conversation Array]
┌────────────────────────────┐                 ┌────────────────────────────┐
│ system : Role directives   │                 │ system : Role directives   │
│ user   : Task instruction  │                 │ user   : Task instruction  │
│ assistant: [Tool Call] ──┐ │ (Extract body)  │ assistant: Previous turn   │
│ tool   : Delivery confirmed│ │               │ tool   : Tool result       │
└────────────────────────────┘ └──────────────>│ user   : [Team Message]    │ <-- Appended to B's
                                               │   "A: Port 80 found"       │     array tail!
                                               └────────────────────────────┘
```

---

## 2. Independent Conversation Graph Structure

Agents do not directly share a monolithic transcript; **each agent maintains its own isolated conversation array**.

- **`main`'s conversation array:** Contains the overarching objective, strategic plans, and synthesized reports from subordinate agents.
- **`worker-01`'s conversation array:** Contains specific reconnaissance directives and verbose tool execution outputs.
- **`worker-02`'s conversation array:** Contains isolated exploitation tasks and targeted execution feedback.

---

## 3. Communication Scenario & `messages` Array Transformations

### Scenario
1. `worker-01` (Recon specialist) discovers a vulnerable service during a scan and relays the finding to `worker-02`.
2. `worker-02` (Exploitation specialist) consumes the finding, executes an exploit to obtain a shell, and submits a final completion report to `main`.

---

### Step 1: `worker-01` Tool Invocation (Outbound Transmission)

The LLM for `worker-01` invokes the `team` tool with `op: "send"`:

```json
{
  "role": "assistant",
  "tool_calls": [
    {
      "name": "team",
      "arguments": {
        "op": "send",
        "to": ["worker-02"],
        "kind": "insight",
        "body": "Found vulnerable Apache Tomcat 9.0.1 on port 8080 (admin:admin credentials valid)"
      }
    }
  ]
}
```

---

### Step 2: Receiver `worker-02`'s Actual API `messages` Array

The runtime extracts the payload, formats it as a durable inbox record, and appends it to `worker-02`'s conversation array as a **`user` turn**.

The exact payload sent on `worker-02`'s subsequent LLM API call:

```json
[
  {
    "role": "system",
    "content": "You are worker-02, an offensive security specialist assigned to exploitation."
  },
  {
    "role": "user",
    "content": "Assigned task: Gain a shell using identified web vulnerabilities."
  },
  {
    "role": "assistant",
    "content": "Awaiting target discovery data from recon worker-01..."
  },
  {
    // Note: The API role is "user", but the text explicitly identifies sender: "worker-01"
    "role": "user",
    "content": "TEAM INBOX MESSAGE (durable; retain until semantically compacted)\n{\n  \"sequence\": 12,\n  \"message\": {\n    \"sender\": \"worker-01\",\n    \"kind\": \"insight\",\n    \"body\": \"Found vulnerable Apache Tomcat 9.0.1 on port 8080 (admin:admin credentials valid)\"\n  }\n}"
  }
]
```

- **Receiver Model Perception:**
  > The LLM for `worker-02` reads the incoming `user` entry, clearly identifies that peer `worker-01` provided actionable intelligence regarding port 8080, and immediately formulates the next exploit action.

---

### Step 3: `worker-02` Completion Report (`team finish`)

Having obtained root access, `worker-02` invokes `team` with `op: "finish"`:

```json
{
  "role": "assistant",
  "tool_calls": [
    {
      "name": "team",
      "arguments": {
        "op": "finish",
        "body": "Obtained root shell via Tomcat WAR deployment exploit. Flag: FLAG{pwned_tomcat}"
      }
    }
  ]
}
```

---

### Step 4: Parent `main`'s Actual API `messages` Array

The runtime bubbles this completion report upward to parent `main` as a **`user` turn**.

The payload sent on `main`'s subsequent LLM API call:

```json
[
  {
    "role": "system",
    "content": "You are main, the team lead. Direct the overall engagement and achieve the goal."
  },
  {
    "role": "user",
    "content": "Compromise target server and extract the flag."
  },
  {
    "role": "assistant",
    "content": "Delegated reconnaissance to worker-01 and exploitation to worker-02."
  },
  {
    // Note: The worker's completion report is injected as sender: worker-02, kind: final
    "role": "user",
    "content": "TEAM INBOX MESSAGE (durable; retain until semantically compacted)\n{\n  \"sequence\": 25,\n  \"message\": {\n    \"sender\": \"worker-02\",\n    \"kind\": \"final\",\n    \"body\": \"Obtained root shell via Tomcat WAR deployment exploit. Flag: FLAG{pwned_tomcat}\"\n  }\n}"
  }
]
```

- **Main Model Perception:**
  > The LLM for `main` inspects the final output and flag from `worker-02`, integrates the result into its brief, and submits the final report to the operator.

---

## 4. Why Are Messages Injected Under the `user` Role?

In the standard OpenAI-compatible LLM message specification:

- `assistant`: Output previously generated by the model itself.
- `tool`: Immediate execution result of a tool call made in the immediately preceding turn.
- **`user`**: **All external stimuli and inputs** entering the model from the outside world (operator, environment, or peer agents).

Because messages from peer agents arrive asynchronously, formatting them as structured `user` messages is the only mechanism that respects standard LLM API framing while maintaining clear attribution.

---

## 5. Architectural Summary

| Concept | Implementation in LLM `messages` Array |
| :--- | :--- |
| **Single Agent** | Exactly one independent `messages` array |
| **Message Transmission (`send`)** | Runtime copies sender's tool argument into receiver's array tail as `user` |
| **Sender Identification** | Explicitly declared inside the `content` body (`"sender": "worker-01"`) |
| **Task Completion (`finish`)** | Worker result bubbles up into parent's array tail as a `user` turn |
| **Semantic Compaction** | Long arrays are compacted; essential facts are synthesized into the `brief` |
