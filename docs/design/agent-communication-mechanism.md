# 에이전트 간 통신 메커니즘 (메시지 배열 관점)

본 문서는 `minimal-agent`에서 에이전트들이 상호 통신하는 원리를 **"LLM API 호출 시 주고받는 대화 배열(`messages: [...]`)"** 관점에서 설명합니다.

---

## 1. 통신의 실체

LLM API 호출의 본질은 **"과거 대화 이력 배열(`messages`)"**을 모델에 통째로 전송하는 것입니다.

따라서 `minimal-agent`에서 **"에이전트 간 통신"**은 다음과 같은 배열 조작으로 동작합니다:

> **발신자(A)가 도구로 출력한 메시지를 중앙 시스템이 가로채서, 수신자(B)의 대화 배열 맨 뒤에 `User` 메시지로 복사해 붙여넣는 것.**

```text
[에이전트 A의 대화 배열]                     [에이전트 B의 대화 배열]
┌────────────────────────┐                 ┌────────────────────────┐
│ system : 내 역할 지침   │                 │ system : 내 역할 지침   │
│ user   : 작업 지시     │                 │ user   : 작업 지시     │
│ assistant: [도구 호출] ──┐ (내용 추출)    │ assistant: 내 이전 답변 │
│ tool   : 전송 완료      │  │              │ tool   : 내 도구 결과   │
└────────────────────────┘  └─────────────>│ user   : [팀 메시지]    │ <-- B의 배열 맨 끝에
                                           │   "A: 80번 포트 발견"   │     새로 추가됨!
                                           └────────────────────────┘
```

---

## 2. 각 에이전트의 독립 대화방 구조

각 에이전트는 서로의 대화 기록을 직접 공유하지 않으며, **독립된 자기만의 대화 배열**을 유지합니다.

- **`main`의 대화 배열:** 전체 목표와 하위 에이전트들의 요약 보고가 쌓이는 배열
- **`worker-01`의 대화 배열:** 정찰 과업과 도구 실행 결과가 쌓이는 배열
- **`worker-02`의 대화 배열:** 공격 과업을 수행하는 독립된 배열

---

## 3. 실제 통신 시나리오 및 `messages` 배열 변화 예시

### [시나리오]

1. `worker-01` (정찰 담당)이 스캔을 통해 취약점을 발견하고 `worker-02`에게 전달
2. `worker-02` (공격 담당)가 이를 받아 쉘 획득 후 `main`에게 완료 보고

---

### Step 1: `worker-01`의 도구 호출 (발신)

`worker-01`의 LLM이 `team send` 도구를 호출합니다.

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
        "body": "8080 포트에서 취약한 Tomcat 9.0.1 발견 (admin:admin 로그인 가능)"
      }
    }
  ]
}
```

---

### Step 2: 수신자 `worker-02`의 실제 API `messages` 배열

시스템이 위 내용을 복사하여 `worker-02`의 대화 배열 맨 끝에 **`user` 턴**으로 추가합니다.

`worker-02`의 다음 LLM API 호출 시 전송되는 전체 페이로드:

```json
[
  {
    "role": "system",
    "content": "당신은 침투 공격 전담 에이전트(worker-02)입니다."
  },
  {
    "role": "user",
    "content": "할당된 과업: 발견된 웹 취약점을 통해 쉘을 획득하라"
  },
  {
    "role": "assistant",
    "content": "정찰팀(worker-01)의 타겟 정보 공유를 대기 중입니다..."
  },
  {
    // ★ role은 규격상 user이지만, content 텍스트 안에 sender: "worker-01"이 명시됨 ★
    "role": "user",
    "content": "TEAM INBOX MESSAGE (durable; retain until semantically compacted)\n{\n  \"sequence\": 12,\n  \"message\": {\n    \"sender\": \"worker-01\",\n    \"kind\": \"insight\",\n    \"body\": \"8080 포트에서 취약한 Tomcat 9.0.1 발견 (admin:admin 로그인 가능)\"\n  }\n}"
  }
]
```

- **수신자 모델의 인지:**
  > `worker-02`의 LLM은 `content` 텍스트를 읽고 **`"동료 worker-01이 8080 포트 정보를 보냈구나"`** 하고 발신자를 정확히 파악한 뒤 공격 작업을 개시합니다.

---

### Step 3: `worker-02`의 완료 보고 (`team finish`)

공격에 성공한 `worker-02`가 `team finish`를 호출하여 작업을 끝냅니다.

```json
{
  "role": "assistant",
  "tool_calls": [
    {
      "name": "team",
      "arguments": {
        "op": "finish",
        "body": "Tomcat 배포 취약점으로 root 쉘 획득 완료. 플래그: FLAG{pwned_tomcat}"
      }
    }
  ]
}
```

---

### Step 4: 부모 `main`의 실제 API `messages` 배열

시스템이 완료 보고를 받아 부모인 `main`의 대화 배열 맨 뒤에 **`user` 턴**으로 추가합니다.

`main`의 다음 LLM API 호출 시 전송되는 페이로드:

```json
[
  {
    "role": "system",
    "content": "당신은 메인 지휘관(main)입니다. 전체 침투 목표를 달성하세요."
  },
  {
    "role": "user",
    "content": "타겟 서버를 장악하고 플래그를 획득하라"
  },
  {
    "role": "assistant",
    "content": "worker-01과 worker-02에게 작업을 위임했습니다."
  },
  {
    // ★ 워커의 완료 보고가 sender: worker-02, kind: final 로 주입됨 ★
    "role": "user",
    "content": "TEAM INBOX MESSAGE (durable; retain until semantically compacted)\n{\n  \"sequence\": 25,\n  \"message\": {\n    \"sender\": \"worker-02\",\n    \"kind\": \"final\",\n    \"body\": \"Tomcat 배포 취약점으로 root 쉘 획득 완료. 플래그: FLAG{pwned_tomcat}\"\n  }\n}"
  }
]
```

- **메인 모델의 인지:**
  > `main`의 LLM은 `worker-02`가 제출한 최종 결과와 플래그를 확인하고, 사용자에게 최종 보고서를 제출합니다.

---

## 4. 왜 `user` 역할로 주입되는가?

LLM API 표준 규격에서:

- `assistant`: 모델 자신이 이전에 뱉은 응답
- `tool`: 모델이 직전 턴에 직접 호출한 도구의 반환값
- **`user`**: 외부 세계(사용자, 환경, 타 에이전트)로부터 모델에게 들어오는 **모든 새로운 외부 자극/입력**

따라서 다른 에이전트가 비동기적으로 보낸 메시지는 수신자 모델 입장에서는 외부에서 도착한 새로운 `user` 입력으로 포장하는 것이 API 규격과 모델 추론 논리에 가장 부합합니다.

---

## 5. 핵심 요약

| 개념                     | LLM `messages` 배열 관점에서의 동작                               |
| :----------------------- | :---------------------------------------------------------------- |
| **에이전트 1개**         | 독립된 `messages` 배열 1개                                        |
| **메시지 전송 (`send`)** | 발신자 도구 인자를 수신자 배열 끝에 `user` 메시지로 복사          |
| **발신자 식별**          | `content` 텍스트 내에 `"sender": "worker-01"` JSON으로 명시       |
| **완료 보고 (`finish`)** | 하위 에이전트 결과가 상위 에이전트 배열 끝에 `user` 턴으로 버블업 |
| **컨텍스트 압축**        | 배열이 길어지면 앞부분을 자르고 핵심만 `system` 프롬프트로 이동   |
