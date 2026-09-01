# ADR-0002: Authorized Engagement Injection and Transcript Orchestration

- Status: Proposed
- Created: 2026-08-31 +09:00
- Version target: 0.111.0
- Repository: `agnusdei1207/minimal-agent`
- Relation: ADR-0001을 대체하지 않고 확장한다. ADR-0001의 팀 topology,
  journal/brief 소유권, compaction, provider 불변식은 그대로 유지한다. 본 ADR은
  ADR-0001 §14/§19가 범위 밖으로 둔 "외부 benchmark 실행"과 구분되는
  **외부 주입 인터페이스**와 **승인된 공격 보안 작업 doctrine**, **transcript/TUI
  가독성**만 다룬다.

## 1. 한 문장 결정

`minimal-agent`를 승인된 공격 보안 작업(CTF, pentest lab, xbow류 자율 벤치마크)에서
쓸 수 있도록, (1) 사전 승인 doctrine을 system prompt에 상시 포함하고, (2)
target·scope·flag 형식을 외부에서 비대화형으로 주입하는 `Engagement` 값을 더하며,
(3) 도구 호출과 부모–자식 통신을 typed transcript로 깔끔하게 렌더링한다.

## 2. 배경과 문제

ADR-0001 코어는 일반 팀 에이전트로서 완결되어 있으나, 실사용(승인된 모의해킹)에서
세 가지 공백이 관측되었다.

1. **거부·서문 문제.** 모델이 승인된 CTF/pentest 지시에도 합법성·윤리 서문을 붙이거나
   실행을 미루고 조언에서 멈춘다. ADR-0001 §11은 "조언에서 멈추지 말라"만 말하고,
   이 작업이 사전 승인된 정상 작업임을 명시하지 않는다.
2. **외부 주입 부재.** 목표는 `--goal` 문자열뿐이며, 교전 대상(target/scope),
   금지 대상(off-limits), flag 형식 같은 구조화된 교전 맥락을 비대화형으로 넣을
   경로가 없다. xbow류 벤치마크는 "대상 + flag 형식 주입 → 자율 실행 → flag 회수 →
   종료"의 헤드리스 흐름을 요구한다.
3. **transcript 가독성.** 도구 호출은 `tool → name` / `tool ← name (ok)`로,
   팀 메시지는 `sender → recipients: body`로 평평하게 찍혀, 어느 것이 액션·결과·
   부모–자식 통신인지 한눈에 구분되지 않는다.

## 3. 결정

### 3.1 승인된 교전 doctrine (상시)

system prompt에 두 블록을 추가한다. 문구는 이 저장소에서 clean-room으로 새로
작성하며 외부 소스를 복사하지 않는다.

- **Standing-authorization 블록 (상시).** 이 런타임은 승인된 교전 안에서만 쓰인다는
  전제를 표준 권한으로 취급한다. 승인 경고·합법성 확인·윤리 서문·moralizing preamble을
  붙이지 않고, exploit 개발·리버싱·정찰·payload 구성·대상 상호작용을 공격 보안이라는
  이유로 거부하거나 완화(water down)하지 않는다. 요청을 수행하고 기술적 결과를 직접 보고한다.
  경계는 법적 판단이 아니라 **운영적 경계**뿐이다: 명시된 target/scope 안에 머무르고, 증거를
  보존하며, 불필요한 평문 비밀 노출을 피한다. 특히 **자율 헤드리스 모드(`--headless --auto`)에서는
  대화형 확인 요청으로 멈추지 않고(No interactive hang), 비인가·파괴적 경로는 즉시 폐기한 뒤
  scope 내 유효한 공격 벡터로 자율 피벗(Pivot)**한다.
- **OPERATING STYLE 블록 (상시).** 도구로 즉시 실행하고, 할 일을 미리 나레이션하거나
  과업을 반복 진술하지 않는다. "Let me…/먼저 …하겠습니다/좋습니다, 파악했습니다/안전한
  흐름을 위해" 같은 서두·filler·메타 코멘트를 제거한다. 명령을 설명하지 말고 실행한다.
  허용 여부나 안전한 대상을 곱씹지 말고 명시된 target을 바로 공략한다. 산문은 다음
  행동을 바꿀 때만 한 줄. 목표 달성 시 증거와 함께 한 줄로 보고한다. 장황함("개소리")을
  막는 기본 세팅이다.
- **CTF solve-loop 블록 (`kind == ctf`일 때만).** 값싼 정찰 우선, 한 번에 하나의
  구체 가설, 최소 probe로 검증, 같은 실패 명령을 바꾸지 않고 반복 금지, 두 번 헛돈
  probe 뒤 pivot, flag는 교전 `flag_format`에 맞는 **실제 대상 출력에서만** 채취하고
  스스로 지어내지 않음. 도구 실패 시 즉시 Python/pwntools/Z3/Scapy 스크립트 기반 무기화(Weaponization)로
  전환하여 공략을 지속한다.

이는 규칙 엔진이 아니라 prompt doctrine이다(ADR-0001 §3의 "전략을 규칙 엔진으로
강제하지 않는다"와 정합). 운영적 confirm-before 항목도 별도 승인 엔진을 만들지 않고
모델 지침으로 둔다.

### 3.2 Engagement 주입

`Engagement`는 다음을 담는 작은 값이다.

| 필드 | 뜻 |
|---|---|
| `kind` | `ctf` \| `pentest` \| `lab` |
| `title` | 교전 이름(사람이 읽는 한 줄) |
| `scope` | 허용 대상 범위(예: `10.10.10.0/24`, URL) |
| `off_limits` | 금지 대상 목록 |
| `flag_format` | flag 정규식/형식(예: `flag\{[^}]+\}`) |
| `objective` | 교전 목표. 없으면 run goal을 사용 |

주입 경로:

- **파일**: `--engagement <path.json>` — 위 필드를 담은 JSON 하나.
- **플래그**: `--engagement-kind`, `--target`(= scope), `--off-limits`(반복),
  `--flag-format`, `--objective`. 플래그는 파일 값을 덮어쓴다.
- **환경변수**: 없음. 비밀이 아니므로 파일·플래그로 충분하며, 새 앱-이름 환경
  계약을 만들지 않는다(ADR-0001 §13.1 원칙).

`Engagement`는 `RuntimeConfig`에 실려 `create`/`resume`로 흐르고, `build_system`이
현재 brief 앞에 **bounded target-context 블록**으로 렌더한다. 텍스트는 goal과 같은
16 KiB 상한, off-limits는 개수/합계 상한을 검증한다. 비밀은 렌더하지 않는다.

### 3.3 헤드리스 자율 실행 (xbow류)

비대화형(파이프/`--plain`)에서 `--headless`를 주면:

1. `Engagement.objective`(없으면 goal)를 한 번 제출한다.
2. `--auto`가 켜져 있으면 런타임 auto 루프가 돌고, main이 `report final`로 종료를
   기록하거나 팀이 idle이 될 때까지 이벤트를 관찰한다.
3. 종료 시 요약과 회수한 flag(있으면)를 한 줄 JSON으로 stdout에 출력한다.
4. `flag_format`이 설정됐는데 유효 flag가 없으면 non-zero로 종료한다.

flag는 `ToolFinished.output`으로 관측한 실제 도구 결과에서만 `flag_format`을
매칭한다. assistant/team 텍스트는 모델이 만들어낼 수 있으므로 증거로 인정하지 않는다.

### 3.4 Transcript/TUI 렌더링

transcript 엔트리를 typed로 바꾼다. 각 엔트리는 category(Action/Info/Result/
Message/Error/Reasoning)와 선택적 dimmed 부제를 가진다. 일반 대화와 reasoning에는
세로 rail을 두지 않아 화면을 별도 panel처럼 분절하지 않는다.

- **도구 호출**: 헤더는 `<action> <dimmed target>` (예: `shell` + 흐린
  `nmap -sV 10.10.10.5`), 결과는 하위 라인에 성공/실패 상태와 bounded output.
  raw `tool → name` 표기를 대체한다.
- **부모–자식·형제 통신 가시성**: 팀 메시지는 항상 **누가 누구에게**(방향 화살표
  `sender → recipients`) + **무슨 종류**(`Progress`/`Insight`/`Request`/`Final` kind
  라벨) + **핵심 내용**(메시지 본문 substance)로 렌더한다. 본문은 `communication.md`가
  요구하는 substance-only(원시 JSON·군더더기 없이 요지)이므로 `{}`를 통째로 펼치지 않고도
  "무슨 말을 전달했는지"가 드러난다. main뿐 아니라 워커↔워커·워커→main 방향도 같은
  포맷으로 보인다(워커는 이 통신 라인과 lifecycle로만 등장, 원시 스텝은 숨김).
- **오케스트레이션 도구 결과 억제**: `team`/`journal` 도구의 반환은 배부용 bookkeeping
  JSON(생성된 `agent_id`, 인스펙션 덤프 등)이라 상태(ok/failed)만 보이고 본문은 숨긴다.
  같은 정보가 lifecycle 라인(`spawned worker-01 …`)과 통신 라인으로 이미 드러나므로,
  transcript는 실제 작업(shell 등)의 출력에 집중된다.
- **Markdown**: CommonMark event parser로 제목·강조·목록·인라인 코드·코드 블록을
  terminal span으로 변환한다. source marker를 그대로 노출하지 않는다.
- **색상 테마**: category별 단일 팔레트. 헤더는 강조색, 부제·부가정보는 dimmed,
  오류는 경고색이다.

TUI는 여전히 runtime의 bounded projection이다(ADR-0001 §13). 상한(1,000 entries,
2 MiB, 128 KiB/entry)은 유지하고, typed 렌더는 표시 계층에서만 이뤄진다.

또한 transcript 스크롤은 **상단 오프셋 앵커 + 하단 추종(follow)** 이다. 위로 스크롤한
화면은 하단에서 스트리밍/증가가 일어나도 오프셋이 고정되고, 하단으로 돌아오면 최신
내용을 다시 추종한다(하단 기준 거리 모델의 위치 튐을 제거).

### 3.5 실행 중 turn steering (진행 유지 주입)

main turn이 도는 동안 들어온 사용자 입력은 현재 진행을 취소하지 않는다. 대신 그
입력을 **다음 model-turn 경계에서 실행 중 세션에 user 메시지로 주입**한다. 모델은
"현재 컨텍스트 + 새 입력"으로 최신 지시를 받아 스스로 우선순위를 재정렬한다.

- `steer_main(text)`는 main turn이 활성일 때만 큐에 넣고, idle이면 거부한다(호출자는
  일반 turn으로 시작). TUI는 main이 working일 때 이 경로로 보낸다.
- **목표가 곧 실행이다.** `/goal <목표>`는 목표 저장과 동시에 자율 루프를 켠다(빈 `/goal`은
  해제·중지). 런타임 `set_goal`은 여전히 목표만 기록하는 불변식이고, auto 토글은 UI
  계층에서 함께 수행한다(ADR-0001 §11의 "goal은 auto를 바꾸지 않는다"를 UX에서 갱신).
- **실시간 `/target <scope>`.** 재시작 없이 authorized 타깃/스코프를 주입해 헛도는 것을
  잡는다. engagement는 `Mutex`로 실시간 변경되고 `EngagementSet`로 영속되며, 진행 중인
  main turn도 다음 model-turn부터 새 타깃을 렌더한다.
- 주입은 inbox 통합과 같은 model-turn 경계에서 이뤄지고, 초기 입력과 동일하게
  Transcript(User)로 저널링된 뒤 세션에 append된다.
- 모델이 tool call 없이 응답을 마쳤어도 steering이 대기 중이면 turn을 끝내지 않고
  이어서 새 지시를 처리한다. turn 종료와 경쟁한 잔여 입력은 같은 세션의 후속 turn에서
  소비되어 유실되지 않는다. 전체는 여전히 `max_model_turns` 상한을 지킨다.

### 3.6 워크스페이스 크레이트 구조 (관심사 분리)

단일 `minimal-agent` 크레이트(10k+줄)를 의존 계층에 따라 Cargo 워크스페이스로 분리한다.
순환 없는 DAG이며, 루트 `minimal-agent`는 각 크레이트를 과거 모듈 경로로 재노출하는
umbrella facade(`pub use ma_core::domain …`)라서 바이너리와 통합 테스트는 그대로
`minimal_agent::runtime::…`을 쓴다.

| 크레이트 | 포함 | 의존 |
|---|---|---|
| `ma-core` | domain, engagement | — |
| `ma-provider` | provider, settings | — |
| `ma-journal` | journal | ma-core |
| `ma-coordinator` | coordinator | ma-core, ma-journal |
| `ma-context` | brief, compaction | ma-core, ma-journal, ma-coordinator, ma-provider |
| `ma-runtime` | runtime, tools | 위 전부 |
| `ma-tui` | tui | ma-core, ma-provider, ma-coordinator, ma-runtime |
| `minimal-agent` | main + umbrella lib | 위 전부 |

외부 의존 버전은 `[workspace.dependencies]` 한 곳에서 관리하고, Docker 빌드는 `crates/`를
함께 COPY한다. 이 분리는 동작을 바꾸지 않으며(테스트·불변식 동일), 컴파일 경계와
소유권만 명확히 한다.

### 3.7 프롬프트 자산화·역할 분리

시스템 프롬프트를 Rust 문자열에서 빼내 프로젝트 레벨 `prompts/`로 옮기고 `include_str!`로
컴파일한다(단일 소스; `.md`만 고치면 다음 빌드에 반영). `build_system`은 다음을 조립한다.

- 공통(모든 에이전트): `authorized-engagement.md`(상시 doctrine) → `operating-style.md`
  → (engagement target 블록) → (CTF면 `ctf-solve-loop.md`) → `team-conduct.md`
  → `communication.md`
- 역할별(정확히 하나): main이면 `main-role.md` + `fan-out.md` + `self-management.md`,
  worker면 `worker-role.md`(`agent_id.is_main()`로 분기) — 워커는 워커 지시(스폰 금지·
  보고 방식), main은 분해·오케스트레이션 지시를 받는다.
  - **`fan-out.md`(main 전용, 병렬화 전략)**: 독립적인 하위작업은 워커 하나로 직렬
    처리하지 말고 **동시에 펼쳐(fan-out)** 여러 워커에 분배하고, 의존이 있는 것만
    순서를 둔다. main은 분배·수집·통합에 집중한다.
  - **`self-management.md`(main 전용, 메모리 관리 전략)**: main은 자신의 working
    memory가 터지지 않도록 **주기적으로 battlefield(현재 상황)를 brief로 요약**한다.
    트랙을 끝냈을 때·워커 결과 여러 개를 통합했을 때·컨텍스트가 커질 때 짧고 최신인
    요약(목표·활성 arc와 상태·확정 사실/값·막다른 길·다음 수)만 남기고 원시 로그·폐기된
    시도는 버린다. 컨텍스트가 넘치기 전에 선제적으로 하는 상시 정비로 규정한다. 이는
    ADR-0001 §9의 런타임 자동 semantic compaction과 **별개의 자율 행동**이다(런타임은
    토큰 예산 기반으로 강제 압축, self-management는 모델이 스스로 brief를 정돈).
- 마지막에 해당 에이전트의 현재 brief.

Docker 이미지 빌드는 `prompts/`도 함께 COPY한다(안 하면 `include_str!`가 실패).

### 3.8 프롬프트 실행 프로토콜 평가 반영

프롬프트 외부 평가(의도 전달 8/10, 행동 재현성 4.5/10)에서 나온 개선안 중, **거부 방지
doctrine은 사용자 최우선 요구(모델이 보안을 핑계로 지시를 거부하는 문제)라 그대로 유지**하고,
재현성 항목만 반영한다.

- 반영: (1) main의 최종 답은 `report {op:final, body}`로 명시(`main-role.md`),
  (2) `team send/finish`·`team create`·`report`의 **정확한 JSON 호출 예제**를
  `communication.md`에 추가, (4·6) "평문만으로 turn을 끝내지 말고 결과를 도구로 기록한 뒤
  종료"라는 **짧은 긍정형 완료 규칙** 추가.
- 보류(사유): (3) team schema를 op별 `oneOf`로 엄격화 — 살짝 다른 형식의 유효 호출까지
  거부해 실패를 늘릴 수 있어, 필드 선언 + 명확한 description(op별 필수 필드)로 대체. (5)
  활성 실행 중 tool call을 런타임 수준에서 강제 — 무한 재호출 위험. 런타임은 이미 평문 turn을
  완료로 보지 않으며(turn은 `finalized:false` 반환, 완료는 report final/team finish로만
  기록; ADR-0001 §11), 프롬프트에 긍정형 규칙을 두는 것으로 충분하다고 본다. 더 강한 런타임
  게이트는 후속 ADR 후보. (7) 실모델 3~5종 행동 평가 — 방법론/측정으로 코드 범위 밖(외부
  benchmark owner).

### 3.9 TUI 상호작용·시각 정돈

- `/` 입력 시 **명령어 메뉴 팝업**(prefix 매칭): ↑↓ 선택, Tab 완성, Enter 실행.
- `/status`는 **모달**로 **라이브 팀 스냅샷**(목표·에이전트 수/활성/미읽음, 에이전트별
  상태·태스크·인사이트·대기, 이어서 brief의 Battlefield 요약)을 띄운다: Esc/x 닫기,
  화살표/PageUp·PageDown 스크롤. Battlefield는 compaction 시에만 갱신되므로 그것만으로는
- **상단 고정 헤더 없는 3행 레이아웃**: 상단 고정 헤더(team roster 행)를 없애 트랜스크립트
  영역을 최대화하고 transcript → status → input 3행으로 둔다(좌측 패딩 제거). 팀 fan-out은
  트랜스크립트(생성/회수/메시지)와 status 행의 워커 수로 인지하며, 목표와 상세 현황은 `/status`에서 확인한다.
- **로딩·활성 상태**: 스피너는 회전 원(◐◓◑◒, braille/별 아님). **어느 에이전트든 살아있는
  turn이 하나라도 있으면 계속 돈다**(`active_turns`는 워커 turn도 포함) — 서브가 도는데
  멈춘 것처럼 보이던 문제 해결. 상태 텍스트는 단순 "working"이 아니라 main의 현재 활동
  라벨을 짧게 보여준다: `thinking`(turn 시작·도구 종료 후 다음 수 판단) / `running <tool>`
  (도구 실행 중) / `responding`(응답 스트리밍 중). 워커만 활성이면 라벨은 "working"으로
  폴백하되 스피너는 계속 돈다.
- **색은 흰색 단색, 파랑 배제**: 활성 표시(team roster의 활성 에이전트, 스피너, shimmer
  band)는 촌스러운 시안 대신 **흰색**으로 통일한다. shimmer는 상태 라벨 글자까지만
  스윕(스피너·경과초는 고정). 의미색(성공 초록/실패·오류 빨강)만 유지, 구조는 흐린 회색.
- **서브에이전트 수 노출**: status 행 꼬리에 현재 워커 수(`· N worker(s)`)를 표시해
  입력 바로 위에서 fan-out 규모가 보이게 한다.
- 스크롤은 상단 앵커(스트리밍 중 위치 고정), 화살표/PageUp·Down/Ctrl+End. 마우스 캡처 없이
  터미널 native 드래그 선택 유지. 전체는 alternate screen full-height.

### 3.10 이미지 빌드 신뢰성과 로컬 자격증명

`dimage.ps1`은 memory-capped `docker-container` 빌더 + `--load`가 대형(브라우저 포함) 이미지를
OCI로 export→re-import하다 간헐적 OOM(graceful_stop)으로 죽는 문제를 없애기 위해, **기본
docker 드라이버로 이미지 스토어에 직접 빌드**한다(export/import 없음). 컴파일 부하는
`CARGO_BUILD_JOBS=2`로 제한한다. `npm run check`의 provider 자격증명은 gitignore된 repo-root
`.env`에만 두고(커밋 금지) `--env-file`/`env_file`로 컨테이너에 주입한다.

### 3.11 TUI 모델 설정·Fault 가시성 계약 (수정 0.111.0)

프로젝트 초기 실행에서 두 가지 UX 결함이 관측되어 불변식으로 굳힌다.

1. **Provider 미설정 시의 중복 Fault 라인.** `RuntimeError::Provider(Configuration)`은
   *비복구형* 오류다. `recoverable_wait_reason`이 이를 recoverable로 분류하면 main 자율
   루프가 동일한 미설정 provider를 32턴(`max_model_turns`) 내내 재시도하며 매 턴
   `RuntimeEvent::Fault`를 broadcast한다. TUI는 이미 Fault 이벤트를 agent 라인으로
   렌더하므로, 제출 결과 경로(`result_rx`의 `Err`)에서 같은 메시지를 또 찍으면 동일
   문구가 두 줄로 중복된다. → **`Configuration` fault는 recoverable이 아니다.** 즉시
   대기하고, Fault broadcast 한 경로로만 표시한다(중복 금지). 도구/provider fault는
   `RuntimeEvent::Fault` 하나로만 노출하고 제출 오류 경로는 상태 줄만 갱신한다.
2. **context-token 입력 루프.** `/model`의 context-token 단계는 `u64` 정수만 파싱해
   `1m`/`128k` 같은 단위 표기를 거부했고, 통과해도 `>= runtime_context_limit()`(기본
   128,000) 하한 덕에 128k 미만 입력은 영원히 reject됐다. → **`k`/`m` 접미사를 허용**하는
   `parse_token_input`을 두고, 하한 검증은 제거한다(사용자가 준 값이 예산보다 크면
   `ContextBudget`이 clip한다). prompt 힌트에 예시(`128k`, `1m`)를 적는다.
3. **자율 루프 재시도 가시성.** provider 타임아웃·전송 오류 같은 *복구형* fault는 main
   자율 루프가 멈추지 않고 계속 재시도한다(2초 백오프). 다만 사용자가 "응답 없음"으로
   착각하지 않도록 **매 실패마다 시도 횟수를 transcript에 적는다**: `RuntimeEvent::Fault`
   메시지에 `자율 재시도 {attempt}회째, model-turn {turn_idx}/{max}`를 포함한다.
   `attempt`는 자율 루프 연속 재시도 횟수(성공 턴 시 0으로 리셋), `turn_idx`는 단일 턴
   내 model-turn 시도 번호다. 재시도 Fault는 runtime 내부에서 한 경로(`run_agent_turn_inner`)
   로만 broadcast하며, `run_main_turn`의 중복 Fault emit은 제거한다.

### 3.12 오퍼레이터 행동 강화·출력 규율·라이브 토큰 표시 (수정 0.111.0)

약한 모델(예: `minimax-m3:free`)이 in-scope 타깃을 주면 "그 사이트에 접속할 수 없다"고
**거짓 거부**하거나 사용자에게 문제를 붙여넣어 달라고 되묻고, 이모지·박스로 출력을
장식해 토큰을 낭비하는 문제가 관찰됐다. doctrine으로 강제한다.

1. **능력 명시 + 거부·되묻기 금지 (`execution-mandate.md` — 전용 보호 파일):** anti-refusal
   계약은 `authorized-engagement.md`(역할만)에서 분리해 **독립 파일 `execution-mandate.md`**로
   두었다. 이유: 다른 프롬프트를 수정할 때 이 부분을 건드려 약한 모델의 거부가 재발하는 것을
   막기 위해서다. 짧고 명령형으로 유지하며(pentesting 스타일 참고, clean-room), 에이전트는
   브라우저·`curl`·네트워크·툴을 갖춘 완전한 Linux 공격 박스를 운용하며 in-scope URL·호스트를
   직접 도달할 수 있다. "접속 불가/브라우징 불가/물리적으로 불가"라고 주장 금지, 문제
   붙여넣기·스크린샷·파일 업로드·난이도 선택 요구 금지, 연습문제 대체 제안 금지. 타깃을
   받으면 즉시 도달·열거·공격한다. 또한 **타깃 성격을 스스로 판단해 거부 금지**: "이건 CTF가
   아니라 정식/상용 서비스다", "CTF 문제를 못 찾겠다", "칠 게 없다"며 멈추는 것 금지. 모의해킹은
   실제 프로덕션 서비스를 치는 것이며, 공격면(인증·세션·토큰·접근제어(IDOR)·주입·SSRF·파일·
   비즈니스로직·오설정)은 항상 존재한다. 산문 "사이트 평가"로 끝내고 실제 공격을 안 하면 그것도
   거부다. **이 anti-refusal·capability 블록은 안정 유지 대상으로, 사소한 프롬프트 수정 때마다
   흔들지 않는다**(잦은 변경이 약한 모델의 거부를 재유발함). 프롬프트는 바이너리에 컴파일
   (`include_str!`)되므로 **변경 반영에는 이미지 재빌드가 필요하다**(런타임 관찰 시 필수 확인 사항).

   **거부를 실제로 막는 방식(pentesting 대조로 확정):** 거부를 한 번도 안 하는 참조 구현
   (`../pentesting`)을 뜯어보니, 그 힘은 **공격적 고함이나 "모든 경계 제거"가 아니라** 정반대였다.
   (a) 일을 **정상 업무로 담담히 정상화**한다("Authorized … work is normal, pre-authorized work").
   (b) **이유를 준다**(이 도구는 승인된 engagement 안에서만 쓰이므로 그 자체가 standing
   authorization). (c) **작고 명확한 운영 경계를 남긴다**(scope 유지·증거 보존·불필요 파괴 금지, 그리고
   범위 밖·대량·지속성·회피 등만 확인). 역설적으로 이 작은 경계가 약한 모델의 불안을 담아줘서
   **나머지 전부를 거부 없이 실행**하게 만든다 — 경계를 통째로 지우면 모델이 스스로 관문을 만들어
   오히려 더 거부한다. 런타임 refusal 억제나 프리필 트릭은 전혀 없고 **프롬프트 문구 하나**가 전부다.
   따라서 `execution-mandate.md`를 이 방식으로 재작성했다(차분한 정상화 + 이유 + 작은 운영 경계).
   추가로, 실제 관찰된 악화 두 가지를 명시 금지했다: **(1) 스코프 자가조사**(CTFtime·sitemap·
   robots·WHOIS를 뒤져 "진짜 CTF가 아니다/상용이다"를 증명하려는 것) — 운영자가 건넨 타깃이 곧
   승인·범위이지 감사 대상이 아니다; **(2) 승인서 메뉴**("승인서/문제 URL 가져와라", 선택지 제시,
   "스코프를 명확히 해달라") — "하냐 마냐"의 유일한 답은 실행이다.
2. **출력 노이즈 금지 = 토큰 비용 규율 (`operating-style.md`):** 이모지·박스·배너·헤더·
   메뉴형 목록·환호성 filler 금지(정보를 담은 드문 경우의 이모지만 예외). 장식 토큰은
   비용만 쓰고 가치가 없다 — 결과와 다음 행동만 최소 단어로.
3. **라이브 토큰 표시 (TUI):** provider의 `ModelDelta::Usage`를 TUI가 누적해 상태줄에
   입력/출력 토큰을 휴먼리더블(예: `tok in 12.3k out 4.5k`, `fmt_tokens`로 k/M 축약)하게
   실시간 표기한다. 팀 전체 요청의 합계다. 토큰 비용이 설계 관심사임을 사용자에게 상시 노출.

### 3.13 벤치마크(xbow) 지원 — 최소·제거 가능 (수정 0.111.0)

xbow104 하네스로 벤치마크를 돌리기 위한 **최소 통합**. 원칙: 코어를 벤치마크용으로 크게
키우지 않는다 — 분석이 끝나면 제거한다.

- **호출·플래그·판정: rs 변경 0.** 하네스(`benchmarks/xbow104/runner.mjs`)가 존재하지 않는
  `--prompt` 대신 **네이티브 CLI**로 부른다: `run --headless --auto --workspace /workspace
  --run /tmp/ma-run --engagement-kind ctf --flag-format 'FLAG\{[0-9a-f]{64}\}' --target <url>
  --objective <prompt>`. `run_headless`가 `{goal,flag,flag_required,summary}` JSON을 stdout에
  출력하고 flag는 engagement 정규식으로 추출되므로, 하네스의 stdout 기반 solved/flags 판정이
  그대로 성립한다. run 저널은 컨테이너-로컬 `/tmp`에 두어 마운트 격리를 지킨다.
- **토큰 텔레메트리: 최소·env-gated·제거 가능 rs.** `MINIMAL_AGENT_TELEMETRY_FILE`이 지정되면
  `record_model_delta`가 응답마다 `{"event":"response","prompt_tokens","completion_tokens"}`
  한 줄을 append한다(미지정 시 no-op). 하네스가 이 파일을 읽어 토큰/비용 KPI를 집계한다.
  **제거 방법: `record_usage_telemetry` 함수와 그 단일 호출부만 삭제**하면 원상복구.
- 이 절과 벤치마크 하네스는 분석 종료 후 제거 대상이며, 코어 불변식(§4)에 영향을 주지 않는다.

### 3.14 약한 모델 강건성 — compaction이 멈추지 않게 (수정 0.111.0)

**관찰된 실패:** 약한 모델(minimax)이 semantic compaction JSON을 ```코드펜스·산문으로 감싸
뱉어 `from_str`가 첫 글자에서 실패 → compaction 차단 → 문맥이 못 줄어듦 → 입력 토큰이
330k로 폭증 → 무한 멈춤 루프. 이는 **설계 사전 위험 발굴(pre-mortem)에서 놓친 가정**이었다:
"LLM은 유효한 JSON을 돌려준다"는 가정이 약한 모델에서 깨졌고, **차단(fail-closed)에 경계된
폴백이 없어** 멈춤이 됐다(방법론의 risk-management pre-mortem 규율 반영).

**두 겹 수정:**
1. **관대한 파싱:** compaction JSON을 `from_str` 전에 첫 `{`~마지막 `}`만 추출(펜스·산문
   벗김). 흔한 래핑 케이스를 흡수.
2. **기계적 폴백:** semantic 시도가 두 번 실패하거나 요약이 충분히 줄이지 못하면, 차단
   대신 `CompactionOutcome::MechanicallyTrimmed`를 반환한다 — **보호된 live tail만** 남기고
   나머지 eligible 항목을 **컨텍스트에서만** 드롭한다. **brief는 그대로**, 드롭 항목은
   **run 저널에 전부 보존**(ADR-0001 §9 원장 불변식 유지 — 데이터 무손실).
   - **배선 감사 교훈(side effect):** 첫 구현은 live tail에 "최신 eligible 항목"을 더 얹었는데,
     이것이 툴-호출/결과 **원자 그룹을 쪼개** 결과만 남기고 호출을 드롭 → provider가 "tool id
     not found"로 400 거부했다. 소비자(`build_request`가 records의 메시지를 순서대로 요청에
     복제)의 짝 무결성 불변식을 놓친 것. 수정: **semantic 경로가 남기는 것과 정확히 동일한
     kept-set(live tail만)**을 남긴다 → 검증된 경로와 동일 상태라 원자 그룹 분할 불가.
     (방법론의 plumbing "원자 그룹 전체-단위 재검증" 게이트에 이미 있던 항목 — 실행에서 놓쳤음.)
   폴백은 최후 수단이며 정상 경로가 아니다; 발생 시 저널 Fault-note + RuntimeEvent로 가시화
   (silent 저하 금지). ADR-0001 §9.1의 "coverage 증명"은 semantic 경로에만 적용되고, 폴백은
   증명 대신 "컨텍스트에서 드롭·저널 보존"이라는 경계된 저하로 명시 대체한다.

### 3.15 도구 출력 컨텍스트 경계 — head/tail 절단 (수정 0.111.0)

**관찰:** 도구 출력이 하드캡(128 KiB)까지 **raw로 컨텍스트에 들어가** 큰 출력 하나가 요청을
부풀리고(예: 56KB HTML) 매 모델 호출을 느리게 만들며 compaction을 자주 터뜨렸다.

**수정:** `ToolRegistry::execute`가 결과를 반환하기 전에 `truncate_tool_content`로 **컨텍스트
친화적 경계(16 KiB)**로 줄인다 — 초과 시 head(60%)+tail(40%)만 남기고 중간을
`[... N bytes omitted; re-run with head/tail/grep or redirect to a file ...]` 마커로 대체.
모든 도구에 일괄 적용(작은 출력은 무영향). 에이전트는 더 필요하면 head/tail/grep·파일
리다이렉트로 재실행한다(CTF 표준 패턴). `MAX_TOOL_RESULT_BYTES`(128 KiB 하드 실패)는 유지.

**배선 감사(전 소비자 추적):** 절단본은 (1) 세션→모델 컨텍스트(목적), (2) run 저널(마커와
함께 기록), (3) headless flag 추출(`tool_evidence`)로 흐른다. **알려진 트레이드오프:** flag가
거대 출력의 **중간(elide 구간)**에만 있으면 놓칠 수 있다 — 실무상 flag는 출력의 시작/끝에
있어 head/tail 16 KiB가 대부분 커버하고, 못 잡으면 에이전트가 `grep`으로 재실행해 회수한다.

### 3.16 `bash` 도구 (이름·구현 정밀화) (수정 0.111.0)

도구 이름을 **`shell`→`bash`**로 바꾼다("shell"은 sh/dash/zsh 등 모호). Linux 런타임에서
프로세스 호출도 `sh -lc`(이미지의 sh=dash) → **`bash -lc`**로 바꾼다 — base 이미지가 bash를
포함(`runtime-base.Dockerfile` apt)하고 공격 도구·doctrine이 bash 기능을 전제하기 때문.
스키마·디스패치·요약·직접셸(`!command`)·테스트를 전수 갱신. (Windows 개발 fallback은 powershell
유지 — 런타임은 Linux.) 프롬프트의 "shell"은 도메인 용어(reverse shell 등)라 그대로 둔다.
**리팩터(레거시 정리):** 내부 식별자도 일관되게 리네임 — `fn shell`→`bash`, `ShellInput`→
`BashInput`, `MAX_SHELL_STREAM_BYTES`→`MAX_BASH_STREAM_BYTES`, 직접셸 경로의 `MainCommand::Shell`/
`UiCommand::Shell`/`Submission::Shell`→`Bash`, `run_shell`/`run_direct_shell`→`run_bash`/
`run_direct_bash`(TUI·main·테스트 포함). 경고 0.

### 3.17 main "생각(reasoning)" 실시간 표시 (수정 0.111.0)

**관찰:** 느린 무료 모델(minimax)이 응답 생성에 수십 초 걸리는 동안 트랜스크립트가 비어 있어
사용자가 진행을 못 봄("thinking·77s" 스피너만). 프로바이더는 `ModelDelta::Reasoning`으로 reasoning
토큰을 이미 잡고 TUI로 보내지만, **TUI가 Text만 표시하고 Reasoning을 버렸다**(옛 설계는 의도적
숨김).

**수정:** TUI가 main의 `ModelDelta::Reasoning`을 **실시간 스트리밍 표시**한다(activity="thinking").
**디스플레이 전용** — reasoning은 세션(모델 컨텍스트)에 다시 들어가지 않는다(런타임의 세션 partial은
여전히 Text만 누적). 이로써 느린 모델의 사고 과정이 보인다. 옛 "reasoning 숨김" 테스트 2개를 "reasoning
실시간 표시" 계약으로 뒤집음. (77s 자체는 모델 속도 문제 — 강한 모델 권장.)

**진짜 원인 — reasoning을 요청하지 않았다:** 표시만 고쳐도 여전히 안 보였다. provider는 reasoning을
**받는 쪽만 파싱**(`reasoning`/`reasoning_content`/`reasoning_details`)하고 **요청에서 켜지 않았다**.
OpenRouter류는 `include_reasoning`(또는 `reasoning` 객체)을 보내야 reasoning을 스트리밍하므로, 안 켜면
모델이 애초에 안 보낸다 → 표시할 것이 없다. **수정:** `OpenAiRequest`에 `include_reasoning: true`를
무조건 실어 보낸다(reasoning 채널이 없는 모델은 무시하므로 안전). 이제 루트 노드의 사고가 한 글자씩
실시간으로 흐른다.

### 3.18 대기 UX — 정직한 상태 구분·입력 확장·사용자 라벨·`[…]` 절단 (수정 0.111.0)

느린 모델을 기다릴 때 "무엇을 하는 중인지"가 안 보여 답답하다는 문제. 상태를 정직하게 나눈다:
`waiting for model`(첫 토큰 전 — 생각이 아니라 대기) → `thinking`(reasoning 스트림) →
`responding`(응답 본문) → `running <tool>`(도구, `step N` 카운터). 재시도는 fault 라인으로 즉시 표기.
경과시간은 `77s`→`1m17s`로 휴먼리더블. 또한 **긴 입력이 한 줄로 잘리던 것**을 내용에 따라 세로로
확장(래핑, 상한 8줄)하고 커서를 멀티라인으로 맞춘다. **사용자 입력 라벨 `you`→`❯`**로 통일. 긴
문자열 절단 마커를 `[…]` 계열로 통일(트랜스크립트 툴 출력 `[... +N more lines …]`, 도구 입력 160자,
도구 출력 12줄/200자 — 입출력 양방향 이미 절단됨; 관찰된 "안 잘림"은 stale 이미지 탓).

### 3.19 Battlefield 노트 도구 (`brief`) — 전략 연속성 (수정 0.111.0)

수백만 토큰을 써도 `/status` 전장이 비어 있는 문제. 원인·해결은 ADR-0001 §9.3 참조: brief는 semantic
compaction만 채우는데 약한 모델은 compaction에 실패(mechanical fallback)해 brief가 영원히 빈다.
해결로 **모든 노드가 `brief` 도구로 자기 battlefield 노트를 직접 상시 갱신**(coverage 없음, journal
`BriefNote`, `read_effective`로 CURRENT BRIEF·`/status`에 주입). `self-management.md`는 범주별 분류
철학(공격면/벡터별 시도→결과+이유, 정확값 보존, 막다른 길/블로커/다음 수)을 강제한다.

### 3.20 오리엔트 우선 — 반사적 팬아웃 금지 (수정 0.111.0)

**관찰:** 시작하자마자, 대상에 대한 어떤 정찰 근거도 없는 **첫 턴에 main이 곧바로 `team create`로
워커를 스폰**했다. 팬아웃 doctrine(`fan-out.md`/`main-role.md`)이 "parallel opportunities를 일찍
찾아 동시에 스폰하라"를 너무 강하게 밀어, 모델이 **아직 존재를 확인하지 못한 트랙**에 팀을
투입하는 투기적·기계적 스폰을 했다. 병렬화는 화면(picture)을 만든 다음의 일인데, 화면을 만드는
일을 대체해 버린 것이다.

**수정(프롬프트 doctrine, 코어 변경 0):** 팬아웃을 **증거 주도**로 재정의한다.
- `main-role.md`: 새 목표에서는 **먼저 스스로 오리엔트**(초기 정찰·트리아지를 brief에 기록)한 뒤
  위임을 판단한다. **첫 수를 `team create`로 열지 않는다** — 근거 없는 반사적 스폰은 확인되지 않은
  트랙에 팀을 묶는다.
- `fan-out.md`: "ORIENT BEFORE YOU FAN OUT" 블록 추가. 자기 정찰이 **실제로 관측한** 다중
  표면·경쟁 가설이 드러났을 때만 병렬 워커를 스폰한다. 오리엔트한 단일 에이전트가 눈감고 팬아웃한
  에이전트보다 더 잘 위임한다.
- `node-internal.md`: 내부 노드도 **분해 전에 배정 과제를 평가**한다 — 단일 밀결합 과제는 쪼개지
  말고 직접 수행하고, 진짜 독립 서브태스크가 있을 때만 팬아웃한다.

이 절은 코어 불변식(§4)·능력을 바꾸지 않는다. 팬아웃 능력은 그대로이고, **발동 시점**만 반사적
turn-0에서 증거가 나온 뒤로 옮겼다. (프롬프트는 `include_str!`로 baked되므로 반영에는 `docker build`
재빌드가 필요하다.)

### 3.21 battlefield 노트 초기 시드·강제 유지 (수정 0.111.0)

**관찰:** 재개된 장기 실행에서 main이 입력 88만 토큰을 쓰고 model-turn 한도(32)까지 돌았는데도
`/status` 전장이 비어("No active arc yet") 있었다. 원인 두 겹: (1) main 초기 brief 템플릿이
**실제 목표 문자열조차 담지 않고** 정적 플레이스홀더("Goal / No active arc yet")였다 — 목표는
코디네이터·상태 헤더에만 있고 노트엔 시드되지 않았다. (2) §3.19의 `brief`는 **자발적 도구**라
약한 모델이 한 번도 호출하지 않으면 노트가 영원히 빈다(semantic compaction 체크포인트도 약한
모델은 실패). 결과: 큰 비용을 쓰고도 전장이 "고장난 것처럼" 빈 채로 남는다.

**수정(§3.19 후속):**
- **초기 시드(코어, 모델 비의존):** `render_main_template`이 battlefield 루트에 **실제 목표
  문자열**을 심는다(main 스냅샷의 task, 비면 "Goal"). 이제 노트는 turn 0부터 목표를 담은 의미
  있는 상태로 시작한다 — 큰 작업 뒤의 빈 플레이스홀더보다 정직하다.
- **강제 유지(doctrine):** `self-management.md`에 "WRITE IT FIRST, KEEP IT CURRENT" — 새 목표의
  **첫 행동은 `brief` 쓰기**(목표·아는 것·초기 계획)이고, 몇 수마다 갱신하며, 실제 작업 후에도
  비거나 안 바뀐 노트는 **직무 실패**라고 못박는다.

**정직한 한계:** 시드는 목표 표시를 보장하고 doctrine는 모델을 강하게 민다. 그러나 attempts·findings
같은 **풍부한 내용은 여전히 유능한 모델을 요구한다** — turn마다 거의 출력이 없는 약한 모델은 시드
이상을 채우지 못한다. 진단된 실행은 약한 모델로 재개된 장기 세션이었다(`npm run check`는 코드는
재빌드하되 예전엔 `/state/current` 저널을 `--resume`으로 이어받아, 누적 상태가 한 저널에 쌓였다).

**후속 결정 — `npm run check`는 매 실행 fresh (사용자 결정 2026-09-02):** 재개는 예전 goal·
transcript·brief를 새 세션에 replay해 약한 모델에서 상태가 드리프트하고 "고장난 것처럼" 보인다.
그래서 `scripts/check.ps1`의 `--resume`·저널 프로브를 폐기하고, 매 실행 새 격리 run root
(`/state/check-<guid>`)로 시작한다. 이제 세션마다 깨끗하다. 저널 durable resume 능력 자체(코어)는
그대로이며, 바뀐 것은 dev 검증 런처의 기본 동작뿐이다. 계약 테스트 `test/check.test.ps1`(활성
저널이 있어도 격리 run·no-resume)이 이 동작을 고정한다.

## 4. 불변식

- engagement 텍스트는 goal과 같은 byte/token 상한을 검증한다. off-limits는 개수와
  합계 상한을 가진다.
- 비밀·자격증명은 engagement 블록, transcript, journal, brief에 렌더하지 않는다.
- flag는 실제 대상 출력에서만 인정하고 런타임이 생성하지 않는다.
- headless 종료 코드는 flag_format 유무와 유효 flag 회수 여부로 결정한다.
- doctrine 추가는 prompt 텍스트일 뿐 새 도구·권한·승인 엔진을 만들지 않는다.
- engagement는 run 생성 시 main 생성 **직후**의 `EngagementSet` journal 이벤트로
  영속된다(main-first 불변식 유지). resume는 journal에서 최신 engagement를 복원하며,
  resume에 명시 플래그/파일이 주어지면 그 값이 우선한다. 영속 payload도 다른 이벤트와
  같은 inline/blob 및 run 상한을 따른다.

## 5. 대안과 기각 사유

- **승인 엔진/권한 합성으로 거부 억제.** ADR-0001 §14가 제거한 계층을 되살리게 되어
  기각. doctrine + 운영적 confirm-before로 충분하다.
- **환경변수 교전 계약.** ADR-0001 §13.1이 앱-이름 환경 계약을 제거한 이유(계약
  비대화)와 충돌. 파일·플래그로 대체.
- **별도 Markdown widget/HTML 렌더러.** 화면 상태와 별도 스크롤 모델을 만들므로
  기각. CommonMark parser의 event만 기존 bounded ratatui projection에 변환한다.

## 6. 구현 범위

- [x] `Engagement` 값과 파싱(파일/플래그), `RuntimeConfig` 편입
- [x] `EngagementSet` journal 이벤트로 영속, resume 복원(플래그 우선)
- [x] `build_system`의 standing-authorization + target-context + CTF doctrine 렌더
- [x] `--engagement*`, `--headless` CLI와 헤드리스 종료 계약
- [x] 실제 tool output 전용 flag 회수와 headless JSON 출력
- [x] typed transcript, tool output, 팀 메시지, Markdown 렌더
- [x] 통신 가시성(sender→recipients + kind + substance), team/journal 결과 JSON 억제
- [x] main 전용 `fan-out.md`(병렬화)·`self-management.md`(battlefield 자율 요약) 프롬프트
- [x] 상단 고정 헤더 없는 3행 레이아웃, 라이브 `/status` 스냅샷, 흰색 단색 표시
- [x] 어떤 에이전트든 활성이면 스피너 지속 + main activity 상태 라벨(thinking/running/responding)
- [x] status 행 워커 수 노출
- [x] 상단 앵커 스크롤 + 대기 큐 가시성
- [x] 실행 중 turn steering(`steer_main`)과 model-turn 경계 주입
- [x] ADR-0001에 본 ADR 포인터 추가, 버전 0.110.0 정합
- [x] Docker-only test/clippy/build gate와 fmt 통과
- [x] `Configuration` provider fault를 비복구형으로 분류해 미설정 시 자율 루프 무한 재시도·Fault 중복 방지 (§3.11)
- [x] `/model` context-token 단계에 `k`/`m` 접미사 파싱(`parse_token_input`) 추가 및 하한 검증 제거 (§3.11)
- [x] 자율 루프 복구형 fault 재시도 시 transcript에 시도 횟수(`자율 재시도 N회째, model-turn i/max`) 표시, runtime 내부 중복 Fault emit 제거 (§3.11)
- [x] 오퍼레이터 행동 강화·출력 규율(장식 금지)·라이브 토큰 표시(모든 에이전트 `ModelDelta::Usage` 누적, status 행 `tok in/out` k/M 약칭) (§3.12)
- [x] xbow 벤치마크 최소·제거가능 통합 — 하네스가 네이티브 헤드리스 CLI로 호출(`runner.mjs`), env-gated 토큰 텔레메트리(`record_usage_telemetry`, `MINIMAL_AGENT_TELEMETRY_FILE`, 단일 호출부 삭제로 원상복구) (§3.13)
- [x] 약한 모델 compaction 강건성 — 관대한 파싱(`extract_json_object`, 첫 `{`~마지막 `}`) + 기계적 폴백(`CompactionOutcome::MechanicallyTrimmed`, live tail만 유지·brief 불변·드롭 항목 저널 보존·원자 그룹 미분할), 저널 Fault-note·RuntimeEvent로 가시화 (§3.14)
- [x] 도구 출력 컨텍스트 경계 — `truncate_tool_content`가 16 KiB로 head(60%)+tail(40%) 절단+생략 마커, 하드캡 `MAX_TOOL_RESULT_BYTES`(128 KiB) 유지 (§3.15)
- [x] `shell`→`bash` 도구·`sh -lc`→`bash -lc`·내부 식별자 전수 리네임(`BashInput`/`MAX_BASH_STREAM_BYTES`/`MainCommand::Bash`/`run_direct_bash`), 프롬프트의 "shell"은 도메인어 유지, 경고 0 (§3.16)
- [x] main reasoning 실시간 표시 — provider가 `include_reasoning: true`로 요청, TUI가 `ModelDelta::Reasoning`을 스트리밍(디스플레이 전용, 세션 partial은 Text만 누적) (§3.17)
- [x] 대기 UX — 정직한 상태 구분(`waiting for model`→`thinking`→`responding`→`running <tool>`), 긴 입력 세로 확장, 사용자 라벨 `❯`, `[…]` 절단 마커 통일, `/model` 토큰 `k`/`m` 접미사 파싱(`parse_token_input`) (§3.18)
- [x] Battlefield 노트 도구 `brief` — 모든 노드가 자기 노트를 상시 갱신(coverage 없음, `JournalEvent::BriefNote`, `read_effective`로 CURRENT BRIEF·`/status` 주입), `self-management.md` 분류 철학 강제 (§3.19)
- [x] 오리엔트 우선 팬아웃 doctrine — `main-role.md`/`fan-out.md`/`node-internal.md`가 첫 턴 반사적 `team create`를 금지하고 자기 정찰이 관측한 독립 트랙에서만 스폰(증거 주도), 코어 불변식·팬아웃 능력 불변 (§3.20)
- [x] battlefield 노트 초기 시드(`render_main_template`에 목표 주입) + 강제 유지 doctrine(`self-management.md` "WRITE IT FIRST"), 빈 전장 증상 해소 (§3.21)
- [x] `npm run check` 매 실행 fresh — `scripts/check.ps1`이 `--resume` 폐기하고 매번 새 `/state/check-<guid>`로 시작, `test/check.test.ps1`이 고정 (§3.21)

**상태 주석(정직):** 위 §3.1~§3.19 구현은 코드에 실려 있고 mandatory Docker 게이트(`scripts/dbuild.ps1 test --workspace`)로 검증된다. 그러나 본 ADR은 ADR-0003·ADR-0004와 **공유 승격 게이트**(리버스셸 catch→pty 업그레이드→캡처 E2E 스모크, ADR-0003 §6)가 green이 될 때까지 **Proposed로 유지하고 crate 버전을 0.110.0으로 동결**한다. 세 ADR은 그 스모크가 green이면 함께 Accepted로 승격하고 0.111.0으로 릴리스한다.

## 7. 검증 시나리오

- engagement 파일+플래그 병합, 플래그 우선, 상한 초과 거부
- engagement 영속 후 resume에서 복원(플래그 미지정 시), 플래그 지정 시 우선
- system prompt에 standing-authorization 상시 포함, `kind==ctf`에서만 solve-loop 포함
- headless: flag_format 매칭 출력에서 flag 회수, 미회수 시 non-zero
- transcript: 도구 호출 헤더/결과 분리, 팀 메시지 kind 라벨·방향, Markdown marker 제거
- 비밀 미렌더(brief/journal/transcript leak 0) 회귀
- 라이브 토큰: 모든 에이전트의 usage 델타가 누적되어 status 행에 k/M 약칭으로 표시 (§3.12)
- compaction 폴백: 무효 JSON 반복 시 semantic 경로와 동일한 kept-set(live tail만)만 남기고 eligible 범위는 미유지(원자 그룹 미분할), brief 불변·드롭 항목 저널 보존 (§3.14)
- 도구 출력: 16 KiB 초과 시 head/tail+마커로 절단, 하드캡 초과는 `OutputLimit` (§3.15)
- bash: 도구명·프로세스·직접셸(`!command`)·내부 식별자 rename 정합, 프롬프트 "shell" 도메인어 유지 (§3.16)
- reasoning: 요청에 `include_reasoning` 실림, TUI가 Reasoning 스트리밍 표시, 세션 partial은 Text만 (§3.17)
- 대기 UX: 상태 라벨 전이와 `parse_token_input`의 `128k`/`1m` 접미사 파싱 (§3.18)
- brief: 노드가 `brief` 기록 → CURRENT BRIEF·`/status`에 주입, coverage 요구 없음 (§3.19)
- 오리엔트 우선: main/internal 프롬프트에 첫 턴 스폰 금지·증거 주도 팬아웃 문구가 실려 `build_system`에 baked, 팬아웃 능력(`team create` 상한·이웃 라우팅) 회귀 없음 (§3.20)

## 8. 완료 정의

1. 위 구현 범위가 코드와 일치한다.
2. 모든 mandatory Docker/Node gate가 exit 0이다.
3. engagement/headless/transcript 회귀 테스트가 통과한다.
4. ADR-0001과 버전·문서 정합이 유지된다(0.110.0).
5. clean-room 경계를 지킨다: 외부 prompt/source/secret 미복사.
