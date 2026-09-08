# ADR-0001: Minimal Autonomous Team Agent Core

- Status: Accepted, implemented and verified
- Created: 2026-08-30 20:19:46 +09:00
- Last modified: 2026-08-31 +09:00
- Version target: 0.110.0
- Repository: `agnusdei1207/minimal-agent`
- 확장: 승인된 교전 주입·CTF doctrine·transcript/TUI 가독성은 ADR-0002가 다룬다.
  topology·통신·resume은 ADR-0004의 후속 결정을 적용한다. 아래 초기 2-depth 및 main 자동 복사 설명은 그 범위에서 대체되었다. journal/brief/compaction/provider의 기본 소유권은 유지한다.

## 1. 한 문장 결정

`minimal-agent`는 main 한 명과 최대 아홉 worker가 독립적인 문맥과 자기 소유
`brief.md`를 갖고, 하나의 내구성 journal을 통해 직접 통신하는 작고 자율적인 Rust
팀 에이전트로 만든다.

RAG, 공유 게시판, 권한 합성, 승인 엔진, 증거 그래프, 별도 검증 서비스는 두지 않는다.
원문은 journal에 보존하고, 모델이 현재 의미만 각 agent의 brief로 큐레이션한다.

## 2. 만들려는 경험

사용자는 복잡한 오케스트레이션 계층을 조작하지 않는다. main에게 목표를 주면 main이
필요한 역할과 업무를 정하고 worker를 만들거나 재배정·회수한다. worker는 부모인 main,
형제 worker와 직접 메시지를 주고받으며 팀처럼 움직인다.

main은 현재 살아 있는 팀의 역할, 업무, 상태, 핵심 통찰을 자기 brief의 작은 Team 및
Battlefield 영역에 유지한다. worker는 자기 시도와 통찰만 자기 brief에 유지한다. 누구도
다른 agent의 semantic brief를 직접 쓰지 않는다.

이 구조의 목적은 “더 많은 계층”이 아니라 다음 세 가지다.

1. 실시간에 가까운 협업
2. 문맥 폭발 없이 의미가 남는 기억
3. 실패해도 원문을 잃지 않고 다시 움직일 수 있는 수명주기

## 3. 설계 철학

- 작은 코어를 우선한다. 새 추상화는 기존 구조로 표현할 수 없을 때만 추가한다.
- 프롬프트/스킬로 해결 가능한 문제를 도구나 모듈 코드 레벨로 가져오지 않는다. 코드는 유지보수·관리 비용과 결함 잠재력이 가장 비싼 자산이므로 코드 최소성을 철저히 지키고, 도메인 행동과 관측 규약은 프롬프트/스킬 계층에 위임한다.
- 모델의 자율성과 의미 이해를 활용하고, 전략을 규칙 엔진으로 강제하지 않는다.
- 데이터 손실 방지와 자원 상한은 전략 강제가 아니라 런타임 불변식으로 둔다.
- raw history와 current meaning을 분리한다. journal은 원장이고 brief는 현재 기억이다.
- 쓰기 소유자는 하나다. shared mutable Markdown은 만들지 않는다.
- 실패는 가능한 한 terminal이 아니다. 재시도 폭주는 막되 새 활동으로 복구할 수 있다.
- 상태와 알림을 혼동하지 않는다. 최신 상태는 보존되고 wake는 힌트일 뿐이다.
- 테스트는 정상 경로뿐 아니라 포화, 중단, 재시작, 부분 실패를 먼저 증명한다.
- 벤치마크 결과는 이 저장소에서 주장하지 않는다. 별도 benchmark owner가 측정한다.

## 4. 용어

| 용어 | 뜻 |
|---|---|
| Run | 하나의 목표를 수행하는 내구성 실행 단위 |
| Main | depth 0의 유일한 팀장 agent |
| Worker | depth 1의 실행 agent. 더 깊은 agent를 만들 수 없음 |
| Live Team | main과 현재 non-terminal worker만 포함한 bounded view |
| Agent Brief | agent 한 명이 소유하는 현재 의미의 Markdown |
| Run Journal | 원문 event와 상태 전이를 보존하는 append-only 원장 |
| Inbox | journal의 미소비 direct message를 recipient별로 투영한 bounded queue |
| Activity Revision | 메시지·재배정·명시적 재개를 잃지 않는 단조 증가 신호 |
| Semantic Compaction | LLM이 source coverage를 증명하며 brief와 live tail로 문맥을 줄이는 작업 |
| Safe Boundary | 모델 호출이나 도구 묶음이 끝나 상태를 일관되게 갱신할 수 있는 지점 |

`team.md`, `shared board`, `memory store`, `evidence ledger` 같은 추가 저장소 이름은
사용하지 않는다. 현재 팀 전장은 main의 하나뿐인 `brief.md` 안에 있다.

## 5. 팀 topology와 수명주기

```text
main (depth 0)
├─ worker-01 (depth 1)
├─ worker-02 (depth 1)
└─ ... 최대 worker 9명
```

- 활성 팀 최대 크기는 main 포함 10명이다.
- main만 worker를 create, assign, recall할 수 있다.
- worker는 다른 worker를 만들 수 없다.
- 역할과 업무는 main이 실행 중 동적으로 정한다.
- recall은 실행 중 provider, semaphore 대기, compaction, tool wait를 취소한다.
- 종료된 worker의 permit과 task handle은 회수한다.
- 종료 이력은 journal과 explicit inspect에 남지만 Live Team과 main prompt에서는 빠진다.
- startup 일부가 실패하면 생성된 worker는 terminal 처리되고 permit과 생성 중인 brief
  projection을 함께 회수한다.

깊이는 `main=0`, `worker=1` 두 단계가 전부다. “2-depth”를 main 아래 worker의 총 두
계층이라는 뜻으로 사용한다.

## 6. 통신

### 6.1 허용 경로

- main → worker
- worker → main
- worker → worker
- 한 메시지의 여러 recipient

메시지는 `Progress`, `Insight`, `Request`, `Final` 네 종류다. `Insight`와 `Final`은
main을 audience에 정확히 한 번 자동 포함한다. 따라서 worker가 중요한 통찰이나 최종
요약을 main에게 따로 복제할 필요가 없다.

### 6.2 저장과 전달

```text
sender
  └─ AgentMessage append + fsync
       └─ recipient Inbox projection
            ├─ activity revision 증가
            └─ sleeping agent wake
```

통신 파일을 agent마다 별도로 만들지 않는다. 메시지는 journal에 한 번 기록되고 각
recipient의 in-memory Inbox로 투영된다. restart에서는 같은 journal을 fold해 미소비
Inbox를 복원한다.

전달 불변식:

- 모든 recipient의 용량과 활성 상태를 먼저 검사한다.
- 검사 후 journal append가 성공해야만 어떤 Inbox도 바뀐다.
- 일부 recipient만 받는 partial delivery는 없다.
- consume acknowledgement가 journal에 기록된 뒤에만 Inbox에서 제거한다.
- recipient는 unread message를 자기 model session에 먼저 편입한 뒤 acknowledgement한다.
  acknowledgement는 unread 보호만 해제하며, 내용은 semantic compaction 전까지 session에 남는다.
- restart는 message와 acknowledgement를 함께 fold해 이미 읽은 메시지도 아직 compaction되지
  않았다면 session에 복원한다.
- terminal sender/recipient로 보내는 orphan message는 기록 전에 거부한다.
- recovery도 live enqueue와 같은 count/byte 규칙을 적용하고 초과 이력은 fail closed한다.
- latest insight는 recipient가 아니라 그 통찰을 만든 sender의 상태에 귀속한다.
- request가 실제로 본 Inbox와 activity revision은 같은 coordinator lock에서 snapshot한다.
  turn 중 뒤늦게 온 활동은 다음 safe boundary에 남고, 이미 본 활동은 중복 wake가 되지 않는다.
- main 생성은 journal 첫 event여야 하고 message ID 중복은 recovery에서 거부한다.

### 6.3 메시지 상한

- body와 insight text 합계: 4 KiB
- 한 agent의 unread count: 64
- 한 agent의 unread payload 합계: 64 KiB
- audience: 활성 팀 크기 이하, 최대 10

포화 시 기존 메시지를 버리지 않는다. 새 send 전체를 typed error로 거부한다.

## 7. 저장 구조와 소유권

```text
runs/<run_id>/
├─ writer.lock
├─ journal/
│  ├─ 0000000000000001.jsonl
│  ├─ 0000000000000002.jsonl
│  └─ ...
├─ blobs/
│  └─ <sha256>
└─ agents/
   ├─ main/
   │  └─ brief.md
   ├─ worker-01/
   │  └─ brief.md
   └─ worker-02/
      └─ brief.md
```

단일 `journal.jsonl`이 무한히 커지는 구조가 아니다. journal은 8 MiB segment로
회전하고, 16 KiB를 넘는 event payload는 SHA-256 blob으로 분리한다. run 전체
journal+blob 상한은 512 MiB이며 마지막 fault를 기록할 reserve를 별도로 둔다. 상한에
도달하면 원문을 삭제하거나 덮어쓰지 않고 journal write를 정지한다.

원장 불변식:

- run당 writer는 하나다.
- sequence는 전역 단조 증가한다.
- record checksum과 blob digest/length를 검증한다.
- 같은 SHA-256 이름의 blob이 이미 있어도 새 record가 참조하기 전에 길이와 digest를 다시
  검증한다.
- 마지막 segment의 partial tail만 복구하고 중간 손상은 fail closed한다.
- tool call만 있고 result가 없는 restart는 side effect를 자동 재실행하지 않는다.
- explicit journal inspect는 최대 1,001 events와 128 KiB response envelope를 함께 지킨다.

brief 소유권:

- main의 semantic 내용은 main session의 검증된 compaction만 바꾼다.
- worker는 자기 brief만 쓴다.
- worker tool call마다 main brief를 다시 쓰지 않는다.
- main driver는 canonical Live Team에서 파생한 runtime block만 교체한다. 이 projection은
  auto off에서도 team activity에 맞춰 갱신되며 semantic 본문은 건드리지 않는다.
- 모든 brief write는 atomic replace이며, checkpoint가 먼저 journal에 기록된다.
- projection read/write/recovery 모두 같은 구조, 의미, token, byte 상한을 검증한다.
- terminal worker의 live brief는 제거하고 explicit inspect에는 명시적인 terminal placeholder를
  반환한다. durable history는 journal에 남는다.

## 8. Agent Brief

### 8.1 Main brief

```markdown
# Main Agent Brief
<!-- bounded runtime Team projection -->
## Goal & Constraints
## Battlefield
## Curated Knowledge
### Facts & Successes
### Hypotheses & Directions
### Dead Ends
## Blockers
## Next Moves
```

Team에는 현재 agent ID, role, current task, state, latest insight, waiting-on만 둔다.
Battlefield는 현재 목표에서 갈라진 큰 작업 arc와 진행 방향을 사람이 읽을 수 있게 짧게
표현한다. raw transcript, payload 목록, 모든 terminal worker 이력은 넣지 않는다.

### 8.2 Worker brief

```markdown
# Worker Agent Brief
## Assignment
## Current State
## Attempts by Domain
## Curated Knowledge
### Facts & Successes
### Hypotheses & Directions
### Dead Ends
## Integrated Messages
## Blockers
## Next Move
```

반복 시도는 payload별 로그가 아니라 도메인 또는 technique 단위로 묶는다. 다음은 짧고
정확하게 남긴다.

- `FACT`: 검증된 사실과 journal reference
- `SUCCESS`: 성공한 값·경로·조건과 journal reference
- `HYPOTHESIS`: 아직 검증되지 않은 의심
- `DIRECTION`: 다음에 확인할 방향
- `DEAD_END`: 반복하지 않아야 하는 이유
- `BLOCKER`: 외부 입력이나 상태 변경 없이는 진행할 수 없는 이유

모델이 “완료했다”고 말한 것만으로 FACT나 SUCCESS가 되지 않는다.

## 9. 문맥과 LLM semantic compaction

각 agent는 독립적인 model session과 context budget을 가진다.

```text
effective = min(configured context, provider context) - response reserve
trigger   = effective의 80%
target    = effective의 50%
brief     = effective의 20% 이하
```

80%에 도달한 해당 agent만 compaction한다. 다른 agent의 session이나 brief는 건드리지
않는다.

### 9.1 반드시 LLM이 하는 일

- source를 의미 단위로 요약·통합한다.
- 반복 시도를 domain/technique로 묶는다.
- exact fact, success, hypothesis, direction, dead end를 구분한다.
- 여러 partition의 부분 요약을 다시 의미 있게 fold한다.
- source digest와 covered range/insight ID를 JSON으로 반환한다.

규칙 기반 head/tail 자르기, 오래된 메시지 삭제, regex 요약, “중요해 보이는 일부만”
선택하는 fallback은 compaction 성공으로 인정하지 않는다.

### 9.2 런타임이 검증하는 일

- source SHA-256가 요청과 같다.
- 모든 required sequence range가 coverage에 들어 있다.
- 모든 required Insight ID가 covered 또는 superseded다.
- unread Inbox, current turn, partial output, incomplete tool은 live tail로 보존된다.
- 한 assistant tool turn과 그 모든 result는 같은 durable atomic group이며 compaction과
  restart에서 분리되지 않는다.
- partition 분할도 trigger와 같은 multilingual-safe token estimator를 사용하며 atomic tool
  group은 크더라도 자르지 않는다.
- 결과가 비어 있지 않고 원래보다 작으며 50% target 이하다.
- brief가 20% token 및 대응 byte envelope 이하다.
- compaction 중에도 공통 provider semaphore, recall, shutdown을 지킨다.
- 한 compaction 전체는 기본 300초 deadline 하나를 공유한다.
- checkpoint는 단일 high-water mark가 아니라 정확한 covered ranges를 기록한다. 따라서
  범위 사이의 live hole은 restart 후에도 사라지지 않는다.
- `RecentExchange` 보호는 매 compaction마다 이전 위치에서 해제하고 최신 completed
  exchange 전체로 이동한다. 오래된 한 turn이 영구 live tail로 남지 않는다.

첫 semantic 응답이 잘못되면 더 작은 partition으로 한 번만 다시 요청한다. 두 번째도
coverage나 감소 조건을 만족하지 못하면 원문과 기존 brief를 그대로 두고
`context curation blocked` Waiting으로 전환한다. 새 활동 또는 명시적 auto 재개 전에는
반복 호출하지 않는다. 설정·직렬화 같은 내부 불변식 오류만 terminal fault다.

### 9.3 Battlefield 노트 — agent가 직접 유지 (`brief` 도구)

semantic compaction만이 brief를 채우는 구조는 약한 모델에서 무너진다. 약한 모델은
coverage JSON을 못 만들어 매번 mechanical fallback으로 빠지고, fallback은 brief를
재생성하지 않으므로 brief가 초기 템플릿("No active arc yet")에 영원히 머문다. 그 결과
수백만 토큰을 써도 전장 상황이 비어 있고, 전략(가설·막다른 길·방법론)이 raw history와
함께 잘려 소실된다.

따라서 전략 연속성의 1차 수단을 compaction에서 분리한다. **모든 노드는 `brief` 도구로
자기 battlefield 노트를 직접·상시 갱신한다.** 이 노트는 coverage 증명이 없고 템플릿
구조를 강제하지 않으므로 약한 모델도 언제나 쓸 수 있다. `BriefNote` journal 이벤트로
durable하며 마지막 쓰기가 이긴다. compaction과 분리되어 mechanical fallback이 지우지
않는다.

- **범주 규율:** 뭉뚱그린 한 덩어리 요약은 금지다(구조가 사라져 전략이 무너지고 이미
  배제한 시도를 반복하게 된다). 공격면·벡터·도메인별로 묶고, 각 시도의 결과
  (working / failed / blocked)와 추상화된 이유를 경계가 드러나게 적는다.
- **정확값 보존:** endpoint, parameter, payload, offset, credential, token, cookie, flag,
  그리고 재현·피벗에 필요한 동작/실패 명령은 원문 그대로 남긴다. 서사만 추상화한다.
- **주입·표시:** 노트가 있으면 그것을 CURRENT BRIEF 주입과 `/status` 전장 패널에 쓴다
  (`read_effective`: 노트 우선, 없으면 coverage-brief/템플릿). 매 turn 재주입되어 raw
  transcript 대신 전략을 이어간다.
- **소유권:** 노트도 소유 agent만 자기 것을 쓴다.

semantic compaction은 context budget을 위해 그대로 존재하되, 이제 전장 상황의
정본은 agent가 유지하는 노트다.

## 10. Provider 장애와 회복

한 논리 model call은 총 deadline 하나를 공유한다. attempt마다 deadline을 새로 만들어
전체 시간이 배수로 늘어나지 않는다.

- 최대 3 attempts
- 429, 5xx, transport는 bounded backoff와 bounded `Retry-After`로 재시도
- 401/403, 402, context overflow, configuration 오류는 즉시 반환
- partial stream이 시작된 뒤에는 같은 intent를 자동 재시도하지 않음
- provider가 delta sender clone을 보관해도 완료된 call의 collector는 call-scoped 종료
  신호로 닫히며 turn을 붙잡지 못함
- `[DONE]`와 finish reason이 모두 없는 partial SSE는 완료로 인정하지 않음
- `length`/`content_filter` 종료도 완전한 답으로 승격하지 않고 partial source로 보존
- 비어 있거나 중복된 tool call ID와 malformed arguments는 실행 전에 invalid response로 거부
- HTTP error body 16 KiB
- model output envelope는 요청 token에서 유도하되 최대 16 MiB
- raw SSE framing은 `Content-Length` 사전 검사와 streaming 누적 검사를 모두 거치며
  64 KiB~32 MiB 사이의 요청별 envelope를 넘으면 parsing 전에 중단한다.
- 사용하지 않는 optional `stream_options`는 강제하지 않는다.
- 명시한 provider context 값이 비어 있거나 0이면 default로 조용히 대체하지 않고
  configuration fault로 반환한다.
- 최종 provider failure만 journal `Fault` 한 건으로 기록

최종 provider failure는 agent를 죽이지 않는다. agent는 짧게 정규화한 waiting reason과
함께 `Waiting`이 된다. 메시지, 재배정, 명시적인 `/auto` 재개가 activity revision을
증가시켜 다시 실행한다. 상태 전달은 최신 값을 보존하는 watch channel을 사용하고 activity
revision을 먼저 발행한다. 같은 watch revision은 한 번만 소비하므로 wake 유실과 중복
후속 턴을 함께 막는다.

자동 retry storm은 금지한다. 같은 장애 상태에서 아무 새 활동도 없으면 request 수는
증가하지 않는다. 아직 Inbox에 남아 있다는 사실도 새 revision이 아니면 재시도 근거가
되지 않는다.

## 11. Auto와 실행 순서

- 모든 session은 명시적 설정이 없으면 auto off다.
- 런타임 `set_goal`은 목표만 저장한다. UI에서 `/goal <목표>`는 목표 저장과 함께 auto를
  켠다(빈 `/goal`은 끈다). ADR-0002 §3.5 참조.
- `/auto`는 autonomous loop을 켜거나 끈다.
- auto off에서도 메시지는 내구성 있게 저장되지만 worker의 최초 provider call은 시작하지
  않는다.
- **auto on에서 main은 목표를 완수(`report final`로 finalize)할 때까지 턴을 이어서
  자율로 수행한다.** 성공적이지만 finalize되지 않은 턴 뒤에는 곧바로 다음 continuation을
  실행한다. autonomous continuation이 **복구 가능한 provider fault**(timeout, rate limit,
  transport)를 만나면 멈추지 않고 **짧은 backoff 뒤 재시도**한다(fault는 transcript에
  보이므로 재시도가 관측 가능). 즉각 재시도(storm)는 backoff로 막고, **비복구 fault**(auth,
  payment, config)나 user-turn fault·recall·shutdown·auto off에서는 멈춰 Waiting으로 두고
  새 activity를 기다린다. worker는 assignment/message 등 activity로 깨어난다.
- user submission FIFO가 autonomous continuation보다 우선한다(같은 turn 경계에서 큐된
  user 명령을 먼저 소비).
- main과 worker의 상태는 provider terminal signal과 임무 완료를 혼동하지 않는다.
- worker 완료는 `team finish`, main 최종 기록은 `report final`처럼 명시적 도구 결과로
  남긴다.
- worker의 plain assistant text는 human transcript에는 보이지만 팀 통신으로 간주하지
  않는다. 중간 지식은 `team send`, 최종 요약은 `team finish`를 사용하도록 prompt에
  명시한다.

동일 실패 행동을 무한 반복하지 않도록 system prompt는 equivalent failure를 도메인으로
묶고, 같은 접근이 연속 실패하면 dead end를 기록한 뒤 pivot 또는 teammate request를
하도록 요구한다. 이는 특정 XSS 풀이 규칙이 아니라 모든 장기 작업에 적용되는 일반
큐레이션 원칙이다.

## 12. 최소 도구와 권한 경계

유지하는 built-in 도구는 다섯 개다.

| 도구 | 목적 |
|---|---|
| `shell` | workspace를 현재 디렉터리로 한 명령 실행 |
| `workspace` | workspace 내부 read/write/list |
| `team` | create/assign/send/wait/inspect/recall/finish |
| `journal` | 명시적인 bounded sequence range inspect |
| `report` | finding 기록, main의 final 기록 |

별도 destructive-action approval engine, role별 tool permission 합성, policy service는 두지
않는다. 모델을 완전한 보안 경계로 취급하지 않기 때문이다. `workspace` 파일 도구의 경로
경계와 process/container isolation은 유지하지만, shell은 실행 프로세스가 가진 OS 권한을
그대로 가진다. 운영자는 신뢰하지 않는 작업을 적절히 격리한 컨테이너에서 실행한다.

자원 상한:

- shell stdout/stderr 각각 128 KiB; 초과는 성공 truncation이 아니라 typed failure
- 모든 built-in tool argument, workspace file read, tool result 각각 128 KiB
- journal range response 128 KiB
- workspace directory list 10,000 entries
- team wait는 spurious wake마다 갱신하지 않는 단일 deadline을 쓰며 recall을 즉시 관찰한다.
  wait result는 ready/count만 반환하고 payload는 다음 request의 durable Inbox에서 정확히 한
  번 전달한다.
- provider call과 compaction은 같은 bounded request semaphore 사용
- user input 64 KiB
- role 256 B, worker task 4 KiB, goal 16 KiB

## 13. TUI와 CLI

TUI는 runtime의 별도 상태 저장소가 아니라 bounded projection이다.

- pending user/async command queue: 8
- 한 개의 FIFO submission driver만 사용
- input 초과나 queue 포화 시 원문을 input에 복원
- bracketed paste는 한 번에 byte 상한을 검사해 전부 넣거나 전혀 넣지 않는다.
- 화면 transcript: 최대 1,000 logical entries 및 2 MiB
- 한 displayed entry와 agent partial stream: 각각 128 KiB
- 화면 clipping은 명시적으로 표시하며 durable journal은 바꾸지 않음
- Live Team에서 빠진 agent의 partial display는 즉시 제거
- broadcast lag는 화면 누락으로 표시하되 journal 손실로 오인하지 않음

지원 command surface:

- `/compact`
- `/new`, `/clear`
- `/goal [objective]`
- `/auto`
- `/model [query]`
- `/config`, `/ce`, `/config-edit`
- `/status`
- `/resume`
- `/agent`, `/agent-<id>`
- `/update`
- `/help`
- `/exit`
- `!<command>`

`/compact`, `/goal`, `!command`는 main FIFO에 실제 연결한다. `/new`, `/resume`, `/update` 중
현재 프로세스 안에 없는 작업만 정확한 restart 또는 npm 명령을 안내한다. `/model`과
`/config`는 핵심 대화 UX이므로 안내문으로 축소하지 않는다.

### 13.1 모델 설정 UX는 런타임 기능이다

모델 설정은 이미지 실행 전 환경변수 계약이 아니라 TUI 안에서 완결되는 사용자 흐름이다.
provider가 아직 없어도 앱과 TUI는 정상 시작하며, 입력을 잃지 않은 채 `/model`로 안내한다.

`/model [query]`는 다음의 한 흐름을 제공한다.

1. OpenAI-compatible provider 또는 custom endpoint 선택
2. 필요한 경우 비밀 입력 모드에서 API key 입력
3. `/models` 조회, query filtering, 실패 시 수동 model ID 입력
4. 선택 결과를 원자적으로 저장하고 새 model turn부터 팀 전체에 적용

활성 turn은 시작할 때 얻은 provider snapshot으로 끝까지 수행한다. 변경 도중인 요청을
중간 provider로 갈아 끼우지 않는다. main과 worker가 각자 별도 설정을 복제하지 않고 단일
`ProviderSlot`을 공유한다. 이 작은 교체 지점 외에 provider service 계층을 만들지 않는다.

설정 우선순위는 명시적 TUI 저장값, 표준 provider 환경변수, 미설정 순서다. 앱 이름이 붙은
`MINIMAL_AGENT_API_KEY`, `MINIMAL_AGENT_MODEL`, `MINIMAL_AGENT_BASE_URL`은 공개 계약에서
제거한다. 호환 입력은 `OPENAI_API_KEY`, `OPENAI_MODEL`, `OPENAI_BASE_URL`과 catalog에
명시된 provider 고유 표준 키만 사용한다. `npm run check`는 key/model 존재를 요구하지 않고
설정 volume을 연결한 뒤 TUI를 연다.

비밀은 TUI transcript, journal, agent brief, team message, 오류 문자열에 기록하지 않는다.
credential과 비밀이 아닌 active provider/model은 run journal 밖의 단일 설정 파일에 저장하고,
임시 파일 작성 후 rename으로 교체한다. 가능한 플랫폼에서는 credential 파일을 사용자 전용
권한으로 제한한다. `/config`는 endpoint와 active model만 표시하며 key는 존재 여부만 보여준다.

plain stdin도 newline을 기다리며 무제한 buffer하지 않고 같은 64 KiB 한도에서 읽는다.
plain mode의 model/compaction/shell 대기 중 Ctrl+C도 즉시 shutdown을 선택할 수 있다. TUI는
Esc로 실행 중인 main turn을 취소하고 Ctrl+C로 입력만 지운다. 일반 `/exit`은 accepted
submission이 끝나기 전 종료하지 않는다.

## 14. 의도적으로 제거한 것

- BM25, dense embedding, vector DB, graph retrieval, prior-run RAG
- shared team Markdown과 worker의 main-note 직접 쓰기
- control plane 및 observation plane hierarchy
- permission profile 합성 및 destructive approval flow
- evidence/lineage/verification 전용 엔진
- 별도 fan-out scheduler와 일회성 delegated task API
- 자동 전략 분류, traversal advisor, completion consensus
- run-wide “compaction ineffective, permanently disable” latch

현재 지식은 journal 원문과 agent-owned curated brief 두 층이면 충분하다. RAG는 “있으면
좋을 것 같다”는 이유로 되돌리지 않는다. bounded brief로 해결되지 않는 실제 검색 recall
실패가 측정되고, 기존 두 층에 자연스럽게 통합되는 최소 인터페이스가 증명될 때만 새 ADR로
검토한다.

여기서 제거한 것은 **코어 저장·검증 계층**이다. 승인된 교전 맥락(target/scope/flag)의
외부 주입 인터페이스와 공격 보안 doctrine은 이 제거 목록에 해당하지 않으며 ADR-0002가
prompt 텍스트와 bounded 값으로 최소 확장한다. 새 권한/검증/승인 엔진은 여전히 두지 않는다.

### 14.1 차세대 취약점 연구(VR) 로드맵과의 경계 (`docs/design/advanced-vulnerability-research-roadmap.md` 연계)

현재 `minimal-agent`는 **Senior Penetration Tester 및 High-Tier CTF Specialist (Level 3)** 수준의 자율 침투 능력을 기준으로 설계 및 최적화되어 있다.

향후 자율 제로데이(0-Day) 탐지 및 심층 취약점 분석(Level 4: Deep Security Analyst, Level 5: Elite Vulnerability Researcher)을 위한 아키텍처 로드맵([`docs/design/advanced-vulnerability-research-roadmap.md`](../design/advanced-vulnerability-research-roadmap.md))에서 다루는 4대 축(Code Property Graph, Headless Decompiler API, Coverage-guided Fuzzing/Triage, Shadow Sandbox Verifier)은 **현재 코어의 범위 밖(Out of Scope)**으로 엄격히 유지한다.

이러한 고도화 기능들은 "미리 코어에 넣어두는 방식"으로 Rust 런타임을 비대화하지 않으며(§3 프롬프트 우선주의 공리), 향후 실제 0-day 연구 환경에서 최소한의 인터페이스로 실익이 입증될 때 별도의 독립 ADR을 통해 단계적으로 채택한다.

## 15. 알려진 trade-off

- journal recovery는 최대 512 MiB run 전체를 검증하므로 큰 run의 restart 비용이 있다.
  이 비용은 무제한이 아니며 원문 보존을 위한 명시적 선택이다.
- terminal agent history는 explicit inspect를 위해 coordinator fold에 남지만 Live Team과
  prompt에서는 제외한다.
- provider catalog는 OpenAI-compatible 범위로 제한한다. native response adapter가 필요한
  provider는 이름만 늘려 가장하지 않으며 별도 결정 전에는 custom endpoint로도 지원을
  주장하지 않는다.
- 모델 목록 endpoint가 없거나 일시 실패하면 수동 model ID 입력으로 계속할 수 있다.
- 별도 verification engine을 제거했으므로 도메인 성공 판단은 model과 명시적 report/tool
  결과에 의존한다. 이 ADR은 성능 점수 향상을 주장하지 않는다.
- LLM compaction이 안전 검증을 통과하지 못하면 진행보다 원문 보존을 우선해 Waiting한다.

## 16. 위험 감사 표

| 위험 | 방어 |
|---|---|
| Inbox 무제한 증가 | count+bytes+audience 상한, append 전 atomic preflight |
| 429 한 번에 worker 사망 | bounded retry 후 non-terminal Waiting |
| retry storm | activity revision 전에는 재호출 없음 |
| off/on wake 유실 | latest-value watch channel + monotonic activity revision |
| 실행 중 auto 변경이 두 번 해석됨 | publish-before-watch + revision 단일 소비 |
| stale unread Inbox가 provider 재시도 유발 | unread 존재와 새 activity revision 분리 |
| team wait payload가 tool result와 Inbox에 중복 | wait는 ready/count만 반환, payload는 Inbox 단일 경로 |
| turn 중 본 activity가 후속 turn을 또 생성 | Inbox+revision atomic snapshot과 last-request revision |
| worker가 report final로 main 전달을 우회 | worker final은 거부하고 `team finish` 단일 경로 사용 |
| worker가 main brief 매번 rewrite | main-only writer, main safe-boundary sync |
| main 상태판이 auto off에서 낡음 | semantic 본문과 분리된 derived runtime projection |
| terminal worker가 prompt를 계속 키움 | bounded Live Team과 durable history 분리 |
| startup 중간 실패가 terminal brief를 남김 | terminal 전환과 projection cleanup을 같은 실패 경로에서 수행 |
| partial SSE를 완료로 오인 | finish marker 필수, partial retry 금지 |
| Content-Length 없는 SSE 메모리 증가 | raw stream 누적 byte envelope |
| 중복 tool call ID가 실행·recovery를 모호하게 함 | provider assembly에서 실행 전 fail closed |
| attempt별 timeout 곱셈 | logical-call total deadline 하나 |
| semaphore 대기 중 recall 무시 | acquire 자체를 cancellation과 select |
| compaction이 request cap 우회 | 일반 call과 동일 semaphore |
| tool wait/shell이 shutdown 무시 | outer cancellation select, kill-on-drop, handle abort fallback |
| provider가 delta sender를 반환 뒤 보관 | call-scoped collector 종료 신호와 queued delta drain |
| agent별 shutdown timeout이 누적 | 모든 driver join이 단일 2초 shutdown deadline 공유 |
| UI Enter마다 task spawn | bounded queue + single submission driver |
| UI partial map에 recalled worker 누적 | Live Team 변경 시 partial retain |
| 큰 role/task가 Team prompt 팽창 | journal 전 bounded text validation, recovery 대칭 검증 |
| tampered brief 무제한 read | bounded read와 구조/의미/token 재검증 |
| journal inspect가 전체 run 적재 | selected range와 response byte envelope |
| output 성공 truncation으로 의미 손실 | 초과 시 typed failure, 원문 journal 보존 |
| 거대한 tool argument가 side effect 전 메모리/IO 확대 | bounded counting serialization 후 128 KiB preflight |
| compaction 누락/환각 | digest, range, Insight coverage, reduction 검증 |
| scalar checkpoint가 중간 hole 삭제 | exact covered ranges만 restart에서 제외 |
| 최신 tool result만 남아 API 문맥이 깨짐 | assistant tool turn과 모든 result를 같은 atomic group으로 보호 |
| 이전 RecentExchange가 영구 고정 | 매 compaction에서 newest completed exchange로 보호 이동 |
| ACK 뒤 메시지가 다음 turn에서 소실 | session 편입 후 ACK, restart 대칭 복원 |
| 통찰이 수신자 상태로 잘못 귀속 | sender owns semantic insight, audience owns delivery |
| crash 뒤 tool side effect 재실행 | incomplete group을 기록만 하고 replay 금지 |
| storage cap에서 원문 삭제 | reserved fault 후 write stop |
| 기존 SHA 이름 blob 변조 | 참조 전 digest와 length 재검증 |
| npm download/pack surface 비대화 | asset 128 MiB, checksum 1 MiB, 120초와 exact pack allowlist |

## 17. 구현 범위와 상태

### 완료된 코드 범위

- [x] Rust 1.98, Node 24 LTS, package version 0.110.0
- [x] flat main/worker domain과 active team cap 10
- [x] direct parent/child/sibling message와 bounded Inbox
- [x] segmented checksum journal, SHA-256 blobs, restart fold
- [x] agent별 하나의 `brief.md`와 main-only runtime projection
- [x] LLM-only semantic compaction과 coverage/reduction gate
- [x] OpenAI-compatible streaming provider와 bounded retry/deadline
- [x] provider/compaction Waiting 및 activity-based resume
- [x] independent Tokio agent drivers와 shared request semaphore
- [x] shell/workspace/team/journal/report tools
- [x] bounded TUI/plain input과 canonical slash command parser
- [x] provider 미설정 TUI 시작과 in-process `/model` provider/key/model selector
- [x] single provider slot hot-swap, atomic settings persistence, secret input masking
- [x] npm launcher, owned runtime-base/app Dockerfiles, compose, capped development wrappers
- [x] `npm run check`의 capped image build → interactive TUI 단일 경로
- [x] pinned Chrome for Testing과 CLI toolset을 포함한 자체 runtime base
- [x] RAG/control-plane/permission/evidence dependencies 없음

### 완료된 검증 및 인계

- [x] Docker-only all-target test
- [x] Docker-only clippy `-D warnings`
- [x] Docker-only release build
- [x] Node 24 launcher/package verification
- [x] capped Docker image build와 runtime smoke
- [x] producer→journal/state→brief/prompt/TUI plumbing 최종 감사
- [x] version, metadata, README, ADR 정합성 확인
- [x] `minimal-agent` main commit/push (`agnusdei1207`)
- [x] `../memory` 개발 방법론에 이번 공백 감사 교훈 반영 후 commit/push

실측 증적은 다음과 같다.

- Docker-only Rust test: 12개 test binary, 121 passed, 0 failed
- Docker-only Clippy all-targets: exit code 0, warning 0
- Docker-only optimized release build: exit code 0
- Node 24 package/launcher: 3 passed, 0 failed
- capped image: manifest list `sha256:72c82e4b6f649713c3868680957bb4bef946dc6df12244b84541186316fb6786`
- container smoke: `minimal-agent 0.110.0`, `linux/amd64`, UID/GID `10001:10001`
- memory methodology: commit `0c48202ba3991472af9ef902012a20f9e16869a7` pushed to `origin/main`

## 18. QA 계약

Rust는 host에서 실행하지 않는다. 모든 build/test/fmt/clippy는 capped Docker wrapper만
사용한다.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dbuild.ps1 fmt --all -- --check
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dbuild.ps1 test --all-targets
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dbuild.ps1 clippy --all-targets -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dbuild.ps1 build --release
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/nverify.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dimage.ps1 -Target all -Tag minimal-agent:qa-0.110.0
```

2026-08-31 Docker delivery evidence:

- capped base and app build: exit 0
- Chrome for Testing `152.0.7977.64`: exit 0
- Nmap `7.98` and Python `3.14.4`: exit 0
- application `minimal-agent 0.110.0`: exit 0
- runtime identity `10001:10001`: exit 0
- Compose required-variable rejection and configured render: observed as designed
- interactive TUI start and canonical `/exit`: exit 0
- provider 환경변수가 없는 빈 state에서 TUI 시작: exit 0
- `/model` custom setup, model lookup fail-soft, runtime activation, `/config`: exit 0
- API key input masking, settings mode `0600`, run journal secret search: leak 0

필수 empirical scenarios:

- main+worker 9와 초과 거부
- worker spawn 금지와 depth 2 이상 거부
- main↔worker 및 sibling direct message
- Inbox count/byte/audience 포화의 journal 증가 0 및 partial delivery 0
- restart 뒤 unread message, waiting reason, brief, session 복원
- active stream, semaphore wait, tool wait, shell 중 recall/shutdown
- provider 429/5xx retry, 402 즉시 종료, partial stream 무재시도, total deadline
- provider fault 뒤 Waiting, request storm 0, message/reassign/auto 재개
- 79% 미발동, 80% LLM compaction, 50% target, coverage 누락 거부
- compaction safe refusal 뒤 원문 보존과 non-terminal Waiting
- worker action 중 main brief single-writer 불변식
- UI queue overflow 입력 복원과 display-only clipping
- 8 MiB rollover, 16 KiB blob, 512 MiB safe stop
- CLI version/help/inspect와 npm launcher checksum contract
- non-root Docker image와 executable version smoke

완료 주장은 실제 command exit code와 test count를 근거로만 한다. 실행하지 못한 gate는
정확한 명령과 이유를 남긴다.

## 19. 배포와 변경 통제

- 모든 작업은 `main`에서 한다. branch, worktree, PR을 만들지 않는다.
- 이 저장소는 public clean-room이다. private pentesting source, prompt, secret, artifact를
  복사하지 않는다.
- benchmark 디렉터리와 외부 benchmark **실행·채점**은 이 구현 범위에서 제외한다.
  단, 외부에서 교전을 주입해 자율 실행하는 **인터페이스**(ADR-0002의 `--engagement`,
  `--headless`)는 제외 대상이 아니다. 벤치마크 대상 구축과 수치 보고는 외부 owner가 맡는다.
- npm/image 설정은 0.110.0을 이어받지만, 외부 publication은 별도 명시적 release 요청
  없이는 수행하지 않는다.
- 구조를 다시 키우는 변경, RAG 복귀, 새 권한/검증 계층, topology 확장은 새 ADR이
  필요하다.

## 20. 완료 정의

다음이 모두 참일 때 ADR-0001 구현을 완료로 표시한다.

1. 위 구현 범위가 코드와 동일하다.
2. 모든 mandatory Docker/Node gate가 exit code 0이다.
3. resource, cancellation, recovery, ownership 회귀 테스트가 통과한다.
4. README, Cargo, npm, Docker version이 0.110.0으로 일치한다.
5. worktree에 의도하지 않은 파일과 benchmark 산출물이 없다.
6. `minimal-agent` origin/main에 maintainer identity로 push된다.
7. 이번에 발견한 설계 공백을 일반화해 `../memory` 개발 방법론에도 남기고 push한다.

이 ADR의 핵심 성공 기준은 코드 양이 아니라, 작은 수의 개념으로 팀 협업·기억·회복을
설명할 수 있고 각 경계가 포화와 실패에서도 같은 규칙을 지키는가이다.
