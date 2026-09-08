# INTENT-0001 · Minimal Autonomous Team Agent Core
- 상태: 완료
- 갱신 2026-09-08

## 왜

지금까지의 자율 에이전트 설계는 문제를 풀 때마다 계층을 늘려 왔다. RAG, 공유 게시판, 권한 합성, 승인 엔진, 증거 그래프, 별도 검증 서비스 같은 것을 코어에 미리 쌓으면 유지보수·관리 비용과 결함 잠재력이 함께 커지고, 정작 팀 협업·기억·회복이라는 본질은 흐려진다. 코드는 가장 비싼 자산이므로, 프롬프트/스킬로 풀 수 있는 문제를 코드 레벨로 끌어오지 않는 것이 이 프로젝트의 제1 공리다.

`minimal-agent`는 그 반대 방향을 택한다. 작은 코어 하나로 팀처럼 움직이는 자율 Rust 에이전트를 만든다. main 한 명과 최대 아홉 worker가 각자 독립된 model 문맥과 자기 소유 `brief.md`를 갖고, 하나의 내구성 journal을 통해 직접 통신한다. 원문은 journal에 append-only로 보존하고, 현재 의미만 모델이 각 agent의 brief로 큐레이션한다. 이 구조가 서면, 처음 보는 사람도 "적은 수의 개념으로 팀 협업·기억·회복을 설명할 수 있는가, 각 경계가 포화·중단·재시작에서도 같은 규칙을 지키는가"라는 두 질문만으로 신뢰를 판단할 수 있다.

이것이 되면 세 가지가 가능해진다.

1. 실시간에 가까운 협업 — worker가 부모 main과 형제 worker에게 직접 메시지를 주고받으며 한 팀처럼 움직인다.
2. 문맥 폭발 없이 의미가 남는 기억 — raw history(journal)와 current meaning(brief)을 분리해, 수백만 토큰을 써도 전략이 소실되지 않는다.
3. 실패해도 원문을 잃지 않고 다시 움직이는 수명주기 — provider 장애나 compaction 실패가 agent를 죽이지 않고, 새 활동으로 복구된다.

설계 철학은 다음과 같다. 새 추상화는 기존 구조로 표현할 수 없을 때만 추가한다. 모델의 자율성과 의미 이해를 활용하되 전략을 규칙 엔진으로 강제하지 않는다. 데이터 손실 방지와 자원 상한은 전략 강제가 아니라 런타임 불변식으로 둔다. 쓰기 소유자는 언제나 하나이며 shared mutable Markdown은 만들지 않는다. 실패는 가능한 한 terminal이 아니다. 상태와 알림(wake)을 혼동하지 않는다. 테스트는 정상 경로뿐 아니라 포화·중단·재시작·부분 실패를 먼저 증명한다. 벤치마크 수치는 이 저장소에서 주장하지 않는다.

## 무엇

사용자는 복잡한 오케스트레이션 계층을 조작하지 않는다. main에게 목표를 주면 main이 필요한 역할과 업무를 실행 중 동적으로 정하고 worker를 만들거나 재배정·회수한다. 관측 가능한 결과는 다음과 같다.

- 사용자는 `/goal <목표>`로 목표를 주고 auto를 켜며, main은 목표를 `report final`로 finalize할 때까지 턴을 자율로 이어 간다. worker는 assignment·message 같은 활동으로 깨어나 실행한다.
- main은 현재 살아 있는 팀의 역할·업무·상태·핵심 통찰과 전장(Battlefield) 상황을 자기 brief의 작은 Team/Battlefield 영역에 유지한다. worker는 자기 시도와 통찰만 자기 brief에 유지한다. 누구도 다른 agent의 semantic brief를 직접 쓰지 않는다.
- worker가 만든 중요한 통찰(`Insight`)과 최종 요약(`Final`)은 자동으로 main을 audience에 정확히 한 번 포함하므로, worker가 이를 main에게 따로 복제할 필요가 없다.
- provider 설정이 아직 없어도 앱과 TUI는 정상 시작한다. 사용자는 입력을 잃지 않은 채 `/model`로 provider·API key·model을 TUI 안에서 완결해 설정하고, 저장 즉시 새 model turn부터 팀 전체에 적용된다.
- provider가 일시 장애(429/5xx/transport)를 내면 agent는 죽지 않고 짧게 정규화한 Waiting reason과 함께 대기하다가, 새 메시지·재배정·명시적 `/auto` 재개로 다시 실행된다.
- 실패·중단·재시작 후에도 미소비 메시지, waiting reason, brief, model session이 같은 journal을 fold해 복원된다. 원문은 어떤 상황에서도 삭제·덮어쓰기되지 않는다.
- TUI는 bounded projection으로 팀 상태·전장 패널(`/status`), agent 전환(`/agent`, `/agent-<id>`), 슬래시 커맨드를 제공한다.

## 수용 기준

- [x] main 포함 활성 팀이 최대 10명이고, 11번째(worker 10번째) 생성은 거부된다 (입력: main+worker 9 상태에서 create → 결과: typed 거부, 팀 크기 불변).
- [x] worker는 하위 worker를 만들 수 없고 depth 2 이상 생성은 거부된다.
- [x] main↔worker 및 sibling worker 간 direct message가 전달되고, `Insight`/`Final`은 main을 audience에 자동 1회 포함한다.
- [x] Inbox의 count(64)/payload(64 KiB)/audience(≤10) 상한 포화 시 새 send 전체를 typed error로 거부하며, journal event 증가 0·partial delivery 0이다.
- [x] restart 후 미소비 Inbox message, waiting reason, agent brief, model session이 journal fold로 복원된다.
- [x] active provider stream, semaphore 대기, tool wait, shell 실행 중 recall/shutdown이 즉시 관찰·취소된다.
- [x] provider 429/5xx/transport는 bounded backoff로 재시도하고, 401/403·402·context overflow·config 오류는 즉시 반환하며, partial stream 시작 후에는 자동 재시도하지 않고, 한 논리 call은 total deadline 하나를 공유한다.
- [x] 최종 provider failure 후 agent는 non-terminal Waiting이 되고, 새 activity revision 없이는 request 수가 늘지 않으며(storm 0), message/reassign/auto로 재개된다.
- [x] 문맥 79%에서는 compaction이 발동하지 않고 80%에서 LLM semantic compaction이 실행되어 50% target 이하로 줄이며, coverage/reduction 조건을 어긴 응답은 거부된다.
- [x] compaction이 안전 검증을 두 번 통과하지 못하면 원문·기존 brief를 그대로 두고 `context curation blocked` non-terminal Waiting으로 전환한다.
- [x] worker action이 일어나도 main brief의 semantic 본문은 main single-writer 불변식을 유지하며, runtime projection만 별도로 갱신된다.
- [x] 각 노드는 `brief` 도구로 자기 battlefield 노트를 상시 durable하게 갱신하고, `read_effective`는 노트가 있으면 노트를 우선 주입한다(없으면 coverage-brief/템플릿).
- [x] journal은 8 MiB segment로 rollover하고 16 KiB 초과 payload는 SHA-256 blob으로 분리하며, run 전체 512 MiB 상한 도달 시 원문 삭제 없이 write를 정지한다.
- [x] built-in 도구는 `shell`/`workspace`/`team`/`journal`/`report` 다섯 개이며, 각 자원 상한 초과는 성공 truncation이 아니라 typed failure다.
- [x] TUI queue 포화·입력 초과 시 원문을 input에 복원하고, 화면 clipping은 명시적으로 표시하되 durable journal은 바꾸지 않는다.
- [x] provider 미설정 빈 state에서 TUI가 시작되고, `/model` custom setup·model lookup fail-soft·runtime activation·`/config` 흐름이 동작한다.
- [x] API key·비밀이 TUI transcript·journal·brief·team message·오류 문자열에 누출되지 않고(leak 0), settings 파일 권한이 `0600`이다.
- [x] CLI version/help/journal inspect와 npm launcher checksum contract가 동작한다.
- [x] non-root Docker image(UID/GID 10001:10001)가 빌드되고 executable version smoke가 통과한다.
- [x] 모든 mandatory Docker/Node gate가 exit code 0이고 README/Cargo/npm/Docker version이 0.110.0으로 일치한다.

## 제약

지켜야 할 것 (런타임 불변식과 상한):

- **팀 크기·topology:** 활성 팀 최대 크기 main 포함 10. main만 worker를 create/assign/recall한다. worker는 다른 worker를 만들 수 없다. 종료된 worker의 permit·task handle은 회수하고, 종료 이력은 journal과 explicit inspect에만 남기고 Live Team·main prompt에서는 뺀다. startup 부분 실패 시 생성된 worker는 terminal 처리하고 permit·생성 중 brief projection을 같은 실패 경로에서 회수한다.
- **메시지 상한:** body+insight text 합계 4 KiB, 한 agent unread count 64, unread payload 합계 64 KiB, audience는 활성 팀 크기 이하 최대 10. 포화 시 기존 메시지를 버리지 않고 새 send 전체를 거부한다.
- **전달 불변식:** 모든 recipient의 용량·활성 상태를 먼저 검사하고, journal append 성공 후에만 어떤 Inbox도 바뀐다. partial delivery 없음. consume acknowledgement가 journal에 기록된 뒤에만 Inbox에서 제거하며, recipient는 unread를 자기 session에 먼저 편입한 뒤 ack한다(ack는 unread 보호만 해제, 내용은 compaction 전까지 session 유지). terminal sender/recipient로의 orphan message는 기록 전 거부한다. recovery도 live enqueue와 같은 count/byte 규칙을 적용하고 초과 이력은 fail closed한다. latest insight는 recipient가 아니라 그 통찰을 만든 sender 상태에 귀속한다. request가 본 Inbox와 activity revision은 같은 coordinator lock에서 snapshot한다. main 생성은 journal 첫 event여야 하고 message ID 중복은 recovery에서 거부한다.
- **저장·원장 불변식:** run당 writer는 하나(`writer.lock`). sequence는 전역 단조 증가. record checksum과 blob digest/length를 검증하며, 같은 SHA-256 이름 blob이 있어도 참조 전 다시 검증한다. 마지막 segment의 partial tail만 복구하고 중간 손상은 fail closed. tool call만 있고 result 없는 restart는 side effect를 자동 재실행하지 않는다. explicit journal inspect는 최대 1,001 events·128 KiB envelope를 지킨다.
- **brief 소유권:** main semantic 내용은 main session의 검증된 compaction만 바꾼다. worker는 자기 brief만 쓴다. main driver는 canonical Live Team에서 파생한 runtime projection block만 교체하고 semantic 본문은 건드리지 않으며, auto off에서도 team activity에 맞춰 갱신한다. 모든 brief write는 atomic replace이고 checkpoint가 먼저 journal에 기록된다. projection의 read/write/recovery 모두 같은 구조·의미·token·byte 상한을 검증한다. terminal worker의 live brief는 제거하고 explicit inspect에는 terminal placeholder를 반환하되 durable history는 journal에 남긴다. battlefield 노트도 소유 agent만 자기 것을 쓰며 `BriefNote` 이벤트로 durable하고 마지막 쓰기가 이긴다.
- **문맥 예산:** `effective = min(configured, provider context) − response reserve`, trigger 80%, target 50%, brief는 effective의 20% token 및 대응 byte envelope 이하. 80%에 도달한 해당 agent만 compaction하고 다른 agent session·brief는 건드리지 않는다. 규칙 기반 head/tail 자르기·오래된 메시지 삭제·regex 요약·"중요해 보이는 일부만" 선택하는 fallback은 compaction 성공으로 인정하지 않는다. unread Inbox·current turn·partial output·incomplete tool은 live tail로 보존하고, assistant tool turn과 그 모든 result는 같은 durable atomic group으로 compaction·restart에서 분리하지 않는다. checkpoint는 scalar high-water mark가 아니라 정확한 covered ranges를 기록해 중간 hole이 restart 후에도 사라지지 않게 한다. 한 compaction 전체는 기본 300초 deadline 하나를 공유하고 공통 provider semaphore·recall·shutdown을 지킨다.
- **provider 회복:** 최대 3 attempts, 한 논리 call은 total deadline 하나를 공유(attempt마다 새로 만들지 않음). 429/5xx/transport는 bounded backoff와 bounded `Retry-After`로 재시도, 401/403·402·context overflow·config 오류는 즉시 반환. partial stream 시작 후 자동 재시도 금지. `[DONE]`·finish reason이 모두 없는 partial SSE는 완료로 인정하지 않고, `length`/`content_filter` 종료도 완전한 답으로 승격하지 않으며 partial source로 보존한다. 비거나 중복된 tool call ID·malformed arguments는 실행 전 거부. HTTP error body 16 KiB, model output envelope는 요청 token에서 유도하되 최대 16 MiB, raw SSE는 `Content-Length` 사전검사와 streaming 누적검사(요청별 64 KiB~32 MiB envelope)를 모두 거친다. provider context 값이 비거나 0이면 default로 대체하지 않고 configuration fault로 반환한다. 최종 provider failure만 journal `Fault` 한 건으로 기록하고 agent는 non-terminal Waiting이 된다. 상태 전달은 latest-value watch channel을 쓰고 activity revision을 먼저 발행하며 같은 revision은 한 번만 소비한다.
- **도구·권한 경계:** built-in 도구는 다섯 개(`shell`/`workspace`/`team`/`journal`/`report`)뿐이다. 별도 destructive-action approval engine, role별 tool permission 합성, policy service를 두지 않는다(모델을 완전한 보안 경계로 취급하지 않기 때문). `workspace` 파일 도구의 경로 경계와 process/container isolation은 유지하되 shell은 실행 프로세스의 OS 권한을 그대로 가지므로, 운영자가 신뢰하지 않는 작업을 격리 컨테이너에서 실행한다.
- **자원 상한:** shell stdout/stderr 각 128 KiB(초과 시 typed failure), 모든 built-in tool argument·workspace file read·tool result 각 128 KiB, journal range response 128 KiB, workspace directory list 10,000 entries, user input 64 KiB, role 256 B, worker task 4 KiB, goal 16 KiB. provider call과 compaction은 같은 bounded request semaphore를 공유한다. team wait는 spurious wake마다 갱신하지 않는 단일 deadline을 쓰고 recall을 즉시 관찰하며, ready/count만 반환하고 payload는 다음 request의 durable Inbox에서 정확히 한 번 전달한다.
- **TUI/CLI 경계:** TUI는 runtime의 별도 상태 저장소가 아니라 bounded projection이다. pending queue 8, 단일 FIFO submission driver, 초과·포화 시 원문 복원, bracketed paste는 byte 상한을 한 번에 검사해 전부/전무로 넣는다. 화면 transcript 최대 1,000 logical entries·2 MiB, 한 entry·agent partial stream 각 128 KiB. 화면 clipping은 명시적으로 표시하되 durable journal은 바꾸지 않는다. 모델 설정은 이미지 실행 전 환경변수 계약이 아니라 TUI 안에서 완결되는 흐름이며, main·worker가 별도 설정을 복제하지 않고 단일 `ProviderSlot`을 공유한다. 활성 turn은 시작 시 얻은 provider snapshot으로 끝까지 수행한다. 설정 우선순위는 명시적 TUI 저장값 → 표준 provider 환경변수(`OPENAI_API_KEY`/`OPENAI_MODEL`/`OPENAI_BASE_URL` 및 catalog가 명시한 provider 표준 키) → 미설정. 앱 이름이 붙은 `MINIMAL_AGENT_*` 키는 공개 계약에서 제거한다. 비밀은 transcript·journal·brief·message·오류에 기록하지 않고, credential과 active provider/model은 run journal 밖 단일 설정 파일에 임시파일-rename으로 원자 저장하며 가능한 플랫폼에서 사용자 전용 권한(`0600`)으로 제한한다.
- **QA·배포:** Rust는 host에서 실행하지 않고 모든 build/test/fmt/clippy는 capped Docker wrapper(`scripts/dbuild.ps1`, `scripts/nverify.ps1`, `scripts/dimage.ps1`)만 사용한다. 완료 주장은 실제 command exit code와 test count로만 한다. 모든 작업은 `main`에서 하고 branch/worktree/PR을 만들지 않는다. 이 저장소는 public clean-room이며 private pentesting source/prompt/secret/artifact를 복사하지 않는다. npm/image 설정은 0.110.0을 이어받되 외부 publication은 명시적 release 요청 없이는 하지 않는다.

## 비범위

이번 코어에서 의도적으로 두지 않는 것:

- BM25·dense embedding·vector DB·graph retrieval·prior-run RAG 등 별도 검색 계층. 현재 지식은 journal 원문과 agent-owned curated brief 두 층으로 충분하다. bounded brief로 해결되지 않는 실제 recall 실패가 측정되고 최소 인터페이스가 증명될 때만 새 결정으로 검토한다.
- shared team Markdown과 worker의 main-note 직접 쓰기, control plane/observation plane hierarchy.
- permission profile 합성 및 destructive approval flow, evidence/lineage/verification 전용 엔진.
- 별도 fan-out scheduler와 일회성 delegated task API, 자동 전략 분류, traversal advisor, completion consensus, run-wide "compaction ineffective, permanently disable" latch.
- 차세대 취약점 연구(VR) 로드맵의 4대 축(Code Property Graph, Headless Decompiler API, Coverage-guided Fuzzing/Triage, Shadow Sandbox Verifier). 현재 코어는 Senior Penetration Tester / High-Tier CTF Specialist(Level 3) 수준의 자율 침투를 기준으로 설계되며, Level 4/5 고도화 기능은 코어를 비대화하지 않고 향후 실익이 입증될 때 별도 독립 결정으로 단계 채택한다.
- benchmark 디렉터리와 외부 benchmark **실행·채점**. 단, 외부에서 교전을 주입해 자율 실행하는 **인터페이스**(INTENT-0002의 `--engagement`, `--headless`)는 제외 대상이 아니다. 벤치마크 대상 구축과 수치 보고는 외부 owner가 맡는다.
- 여기서 제거한 것은 코어 저장·검증 계층이다. 승인된 교전 맥락(target/scope/flag)의 외부 주입 인터페이스와 공격 보안 doctrine은 이 제거 목록에 해당하지 않으며 INTENT-0002가 prompt 텍스트와 bounded 값으로 최소 확장한다. 새 권한/검증/승인 엔진은 여전히 두지 않는다.

## 열린 질문 → 결정

이 코어를 세우며 내린 되돌리기 어려운 결정들:

- **결정:** 지식은 단일 내구성 journal(원문 append-only 원장) + agent-owned brief(현재 의미) 두 층으로만 관리한다 — 원문 보존과 문맥 축소를 분리해야 실패 후에도 원문을 잃지 않고 문맥 폭발을 막을 수 있기 때문.
- **결정:** brief는 single-owner다. 각 agent는 자기 brief만 쓰고 shared mutable Markdown은 만들지 않는다 — 쓰기 소유자를 하나로 두어야 동시성·정합성 불변식이 단순해지기 때문.
- **결정:** 문맥 축소는 LLM semantic compaction만 인정한다(coverage/reduction gate로 검증). 규칙 기반 자르기·삭제·regex fallback은 성공으로 보지 않는다 — 전략(가설·막다른 길·방법론)이 mechanical 절단으로 소실되지 않게 하기 위함.
- **결정:** semantic compaction만으로는 약한 모델에서 brief가 템플릿에 영원히 머무는 실패가 있으므로, 전략 연속성의 1차 수단을 compaction에서 분리해 모든 노드가 `brief` 도구로 자기 battlefield 노트를 상시 직접 갱신하게 한다. 노트는 coverage 증명·템플릿 구조를 강제하지 않고, 공격면·벡터·도메인별로 결과(working/failed/blocked)와 추상화된 이유를 경계가 드러나게 적으며, endpoint·parameter·payload·offset·credential·token·flag 등 재현·피벗에 필요한 정확값은 원문 그대로 보존한다(서사만 추상화). CURRENT BRIEF 주입과 `/status` 전장 패널은 노트를 우선한다.
- **결정:** 활성 팀은 main 포함 10명(main 1 + worker ≤9)으로 상한을 둔다 — 자원 상한과 prompt 팽창을 런타임 불변식으로 막기 위함.
- **결정:** 초기 topology는 `main=0`, `worker=1`의 2-depth이며 통신 허용 경로는 main→worker, worker→main, worker→worker이고 한 메시지는 여러 recipient를 가질 수 있다. 메시지는 `Progress`/`Insight`/`Request`/`Final` 네 종류이며 `Insight`·`Final`은 main을 자동 포함한다. (topology·통신·resume의 후속 확장 결정은 INTENT-0004가 적용한다. journal/brief/compaction/provider의 기본 소유권은 본 문서가 정본으로 유지한다.)
- **결정:** 통신 파일을 agent마다 두지 않고 메시지를 journal에 한 번 기록한 뒤 각 recipient의 in-memory Inbox로 투영한다. restart는 같은 journal을 fold해 복원한다 — 저장 정본을 하나로 두어 partial delivery·중복 wake·유실을 원천 차단하기 위함.
- **결정:** 실패는 terminal이 아니다. 최종 provider failure는 agent를 non-terminal Waiting으로 두고 새 activity(message/reassign/`/auto`)로만 재개한다. 자동 retry storm은 금지하고, unread Inbox 존재만으로는 재시도 근거가 되지 않는다 — 상태(보존)와 wake(힌트)를 혼동하지 않기 위함.
- **결정:** auto on에서 main은 목표를 `report final`로 finalize할 때까지 자율로 턴을 이어가며, 복구 가능한 fault는 짧은 backoff 뒤 재시도하고 비복구 fault·recall·shutdown·auto off에서 멈춘다. worker 완료는 `team finish`, main 최종 기록은 `report final`처럼 명시적 tool 결과로만 남기고, worker의 plain assistant text는 팀 통신으로 간주하지 않는다. 동일 실패의 무한 반복은 equivalent failure를 도메인으로 묶고 dead end 기록 후 pivot/teammate request로 막는다.
- **결정:** built-in 도구는 `shell`/`workspace`/`team`/`journal`/`report` 다섯 개로 고정하고 별도 approval engine·permission 합성·policy service를 두지 않는다. 보안 경계는 모델이 아니라 파일 경로 경계와 process/container isolation, 그리고 운영자의 컨테이너 격리로 확보한다.
- **결정:** 모델 설정은 런타임 기능이다. provider 미설정 상태에서도 앱·TUI가 시작하고 `/model` 흐름으로 provider·key·model을 TUI 안에서 완결하며, 단일 `ProviderSlot` hot-swap으로 팀 전체에 적용한다. 앱 이름 붙은 환경변수 계약을 제거하고 표준 provider 키만 호환 입력으로 받는다.
- **결정:** journal은 8 MiB segment rollover + 16 KiB blob 분리 + run 전체 512 MiB 상한을 가지며, 상한 도달 시 원문을 삭제·덮어쓰지 않고 write를 정지한다(마지막 fault 기록용 reserve 별도). 원문 보존을 진행보다 우선한다.
- **결정:** 승인된 교전 주입·CTF doctrine·transcript/TUI 가독성 확장은 INTENT-0002가, topology·통신·resume의 후속 결정은 INTENT-0004가 다룬다. 본 코어는 그 확장들의 기반(journal/brief/compaction/provider 소유권과 자원·회복 불변식)을 정의한다.
- **결정:** 알려진 trade-off를 수용한다 — journal recovery가 최대 512 MiB 전체를 검증하므로 큰 run의 restart 비용이 있으나 이는 원문 보존을 위한 명시적 선택이다. provider catalog는 OpenAI-compatible 범위로 제한하고 native adapter가 필요한 provider는 이름만 늘려 가장하지 않는다. 별도 verification engine을 제거했으므로 도메인 성공 판단은 model과 명시적 report/tool 결과에 의존하며, 이 프로젝트는 성능 점수 향상을 주장하지 않는다.

---

## AI 판정

| 수용 기준 | 증거(테스트·명령·수치) | 판정 |
|---|---|---|
| 팀 크기 상한과 초과 거부 | mandatory 시나리오 "main+worker 9와 초과 거부"; Docker-only Rust test 12 test binary, 121 passed, 0 failed | 통과 |
| worker spawn 금지·depth 2 거부 | 시나리오 "worker spawn 금지와 depth 2 이상 거부"; 동일 test suite green | 통과 |
| main↔worker·sibling direct message, Insight/Final 자동 main 포함 | 시나리오 "main↔worker 및 sibling direct message"; 통신 fold 테스트 green | 통과 |
| Inbox 포화 거부·partial delivery 0 | 시나리오 "Inbox count/byte/audience 포화의 journal 증가 0 및 partial delivery 0" | 통과 |
| restart 복원(unread/waiting/brief/session) | 시나리오 "restart 뒤 unread message, waiting reason, brief, session 복원" | 통과 |
| recall/shutdown 즉시 취소 | 시나리오 "active stream, semaphore wait, tool wait, shell 중 recall/shutdown" | 통과 |
| provider retry/즉시반환/무재시도/total deadline | 시나리오 "provider 429/5xx retry, 402 즉시 종료, partial stream 무재시도, total deadline" | 통과 |
| provider fault 후 non-terminal Waiting·storm 0 | 시나리오 "provider fault 뒤 Waiting, request storm 0, message/reassign/auto 재개" | 통과 |
| compaction 발동·target·coverage 거부 | 시나리오 "79% 미발동, 80% LLM compaction, 50% target, coverage 누락 거부" | 통과 |
| compaction safe refusal 후 원문 보존 | 시나리오 "compaction safe refusal 뒤 원문 보존과 non-terminal Waiting" | 통과 |
| main brief single-writer | 시나리오 "worker action 중 main brief single-writer 불변식" | 통과 |
| battlefield 노트 상시 갱신·read_effective 우선 | producer→journal/state→brief/prompt/TUI plumbing 최종 감사; brief plumbing green | 통과 |
| journal rollover/blob/512 MiB safe stop | 시나리오 "8 MiB rollover, 16 KiB blob, 512 MiB safe stop" | 통과 |
| 5개 도구·자원 상한 typed failure | 시나리오 group과 output truncation→typed failure; test suite green | 통과 |
| TUI queue overflow 복원·display-only clipping | 시나리오 "UI queue overflow 입력 복원과 display-only clipping" | 통과 |
| provider 미설정 시작·/model 흐름 | Docker delivery: 빈 state TUI 시작 exit 0; `/model` custom setup·lookup fail-soft·runtime activation·`/config` exit 0 | 통과 |
| secret 누출 0·settings 0600 | Docker delivery: API key masking, settings mode `0600`, run journal secret search leak 0 | 통과 |
| CLI·npm launcher checksum | 시나리오 "CLI version/help/inspect와 npm launcher checksum contract"; Node 24 package/launcher 3 passed, 0 failed | 통과 |
| non-root image·version smoke | capped image manifest `sha256:72c82e4b6f649713c3868680957bb4bef946dc6df12244b84541186316fb6786`; container smoke `minimal-agent 0.110.0`, `linux/amd64`, UID/GID `10001:10001` | 통과 |
| 모든 gate exit 0·version 0.110.0 정합 | Docker test/clippy(`-D warnings`, 0 warning)/release build exit 0; Chrome `152.0.7977.64`, Nmap `7.98`, Python `3.14.4` exit 0; README/Cargo/npm/Docker 0.110.0 일치 | 통과 |

**구조 지도 변경:** flat main/worker 도메인(active team cap 10), 단일 내구성 journal(segmented checksum + SHA-256 blob + restart fold), agent별 `brief.md`와 main-only runtime projection, LLM-only semantic compaction(coverage/reduction gate), OpenAI-compatible streaming provider(bounded retry/deadline, 단일 `ProviderSlot` hot-swap), independent Tokio agent drivers + shared request semaphore, 5개 built-in tool, bounded TUI/plain input + canonical slash command parser, npm launcher + owned runtime-base/app Dockerfiles + compose가 코어 모듈로 존재한다. RAG/control-plane/permission/evidence 의존성은 없다.

**남은 것:** topology·통신·resume의 후속 확장은 INTENT-0004, 승인된 교전 주입·CTF doctrine·TUI 가독성 확장은 INTENT-0002로 이관되었다. 벤치마크 대상 구축·수치 보고는 외부 owner 몫이다.

**남는 위험:** 큰 run(≤512 MiB) restart 검증 비용, provider catalog의 OpenAI-compatible 한정, 별도 verification engine 부재로 인한 도메인 성공 판단의 model·report 의존 — 모두 원문 보존·코드 최소성을 위한 명시적 trade-off로 수용됨(§열린 질문 → 결정 참조).

**발견한 부채:** semantic compaction 단독으로는 약한 모델에서 brief가 초기 템플릿에 머무는 공백이 확인되어, battlefield 노트(`brief` 도구, `BriefNote` 이벤트)를 전략 연속성의 1차 수단으로 분리해 흡수했다. 이 공백 교훈은 공통 개발 방법론 저장소에도 일반화해 반영·push했다(memory methodology commit `0c48202ba3991472af9ef902012a20f9e16869a7`).
