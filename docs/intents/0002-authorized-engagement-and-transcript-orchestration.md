# INTENT-0002 · Authorized Engagement Injection and Transcript Orchestration

- 상태: 완료
- 작성 2026-08-31 / 갱신 2026-09-08

## 왜

`minimal-agent` 코어(INTENT-0001)는 일반 팀 에이전트로서 완결되어 있으나, 실제
사용처인 **승인된 공격 보안 작업**(CTF, pentest lab, xbow류 자율 벤치마크)에서 세
가지 공백이 관측되었다.

1. **거부·서문 문제.** 모델이 사전 승인된 CTF/pentest 지시에도 합법성·윤리 서문을
   붙이거나, 실행을 미루고 조언에서 멈춘다. 약한 모델(예: `minimax-m3:free`)은
   in-scope 타깃을 줘도 "그 사이트에 접속할 수 없다"고 거짓 거부하거나, 문제를
   붙여넣어 달라고 되묻고, "이건 CTF가 아니라 상용 서비스다"라며 스스로 관문을 세운다.
   INTENT-0001은 "조언에서 멈추지 말라"만 말하고 이 작업이 사전 승인된 정상 작업임을
   명시하지 않는다.
2. **외부 주입 부재.** 목표는 `--goal` 문자열뿐이며, 교전 대상(target/scope), 금지
   대상(off-limits), flag 형식 같은 구조화된 교전 맥락을 비대화형으로 넣을 경로가
   없다. xbow류 벤치마크는 "대상 + flag 형식 주입 → 자율 실행 → flag 회수 → 종료"의
   헤드리스 흐름을 요구한다.
3. **transcript 가독성.** 도구 호출은 `tool → name` / `tool ← name (ok)`로, 팀
   메시지는 `sender → recipients: body`로 평평하게 찍혀, 무엇이 액션·결과·부모–자식
   통신인지 한눈에 구분되지 않는다.

이것이 되면 승인된 교전에서 약한 모델로도 거부 없이 자율 실행하고, 외부에서
비대화형으로 교전 맥락을 주입하며, 사람이 트랜스크립트에서 무슨 일이 일어나는지
읽을 수 있다.

## 무엇

- 사전 승인 doctrine을 system prompt에 **상시** 포함해, 승인된 교전 안에서는 경고·
  서문 없이 즉시 실행하고 기술적 결과를 직접 보고한다.
- `target`·`scope`·`flag_format`을 담은 `Engagement` 값을 파일·플래그로 비대화형
  주입하고, 헤드리스 자율 실행에서 실제 도구 출력으로부터 flag를 회수해 종료 코드로
  성패를 알린다.
- 도구 호출과 부모–자식·형제 통신을 typed transcript로 렌더해, 액션·결과·메시지가
  구분되고 누가 누구에게 무슨 종류의 무슨 내용을 보냈는지 드러난다.
- 실행 중 turn steering, 라이브 토큰 표시, 상태 라벨 등 대기 UX를 정직하게 보인다.
- 약한 모델에서도 compaction·도구 출력·프롬프트 자산이 무너지지 않도록 코어를 굳힌다.

이 문서는 INTENT-0001의 팀 topology, journal/brief 소유권, compaction, provider
불변식을 그대로 유지하며 확장한다. INTENT-0001 §14/§19가 범위 밖으로 둔 "외부
benchmark 실행"과 구분되는 **외부 주입 인터페이스**, **승인된 공격 보안 작업
doctrine**, **transcript/TUI 가독성**만 다룬다.

## 수용 기준

- [x] engagement 파일과 플래그를 병합하면 플래그가 파일 값을 덮어쓰고, 상한(goal과
  같은 16 KiB, off-limits 개수/합계)을 초과하면 거부된다.
- [x] engagement가 run 생성 시 `EngagementSet` journal 이벤트로 영속되고, resume에서
  복원되며, resume에 플래그/파일이 주어지면 그 값이 우선한다.
- [x] system prompt에 standing-authorization 블록이 상시 포함되고, `kind == ctf`일
  때만 CTF solve-loop 블록이 포함된다.
- [x] 헤드리스 실행이 `flag_format`에 매칭되는 **실제 도구 출력**에서 flag를 회수하고,
  `flag_format`이 설정됐는데 유효 flag가 없으면 non-zero로 종료한다.
- [x] transcript가 도구 호출 헤더/결과를 분리하고, 팀 메시지를 방향(sender →
  recipients)·kind 라벨·substance로 렌더하며, Markdown source marker를 제거한다.
- [x] 비밀·자격증명이 engagement 블록·transcript·journal·brief 어디에도 렌더되지
  않는다(leak 0 회귀).
- [x] 모든 에이전트의 usage 델타가 누적되어 status 행에 `tok in/out` k/M 약칭으로
  표시된다.
- [x] compaction이 무효 JSON에도 멈추지 않는다: 관대한 파싱 후에도 실패하면 semantic
  경로와 동일한 kept-set(live tail만)을 남기는 기계적 폴백으로 컨텍스트를 줄이고,
  brief는 불변, 드롭 항목은 저널에 보존된다(원자 그룹 미분할).
- [x] 도구 출력이 16 KiB 초과 시 head(60%)+tail(40%)+생략 마커로 절단되고, 하드캡
  128 KiB 초과는 `OutputLimit`으로 실패한다.
- [x] 도구 이름·프로세스 호출·직접셸·내부 식별자가 `shell`→`bash`로 정합되게
  리네임되고, 프롬프트의 도메인어 "shell"은 유지되며 경고 0이다.
- [x] provider가 `include_reasoning: true`로 요청하고, TUI가 main의 reasoning을
  실시간 스트리밍 표시하되 세션 partial은 Text만 누적한다.
- [x] 대기 상태가 `waiting for model`→`thinking`→`responding`→`running <tool>`로
  정직하게 전이하고, `parse_token_input`이 `128k`/`1m` 접미사를 파싱한다.
- [x] 모든 노드가 `brief` 도구로 자기 battlefield 노트를 상시 갱신하고, 그 노트가
  CURRENT BRIEF·`/status`에 주입된다(coverage 요구 없음).
- [x] main/internal 프롬프트가 첫 턴 반사적 `team create`를 금지하고, 자기 정찰이
  관측한 독립 트랙에서만 워커를 스폰한다(팬아웃 능력·상한 회귀 없음).
- [x] `max_model_turns` 기본값이 무제한(`usize::MAX`)이고 `0`도 무제한으로 취급되며,
  CLI `--max-turns`와 환경변수 `MINIMAL_AGENT_MAX_MODEL_TURNS`가 `0`/`unlimited`/
  `infinite`를 무제한으로 파싱한다.

## 제약

이 절은 §3.1~§3.22의 모든 영구 결정과 §4 불변식을 흡수한다. 하나라도 잃으면 실패다.

### 승인된 교전 doctrine (상시)

- **문구는 clean-room으로 새로 작성한다.** 외부 소스를 복사하지 않는다.
- **Standing-authorization 블록(상시).** 이 런타임은 승인된 교전 안에서만 쓰인다는
  전제를 표준 권한으로 취급한다. 승인 경고·합법성 확인·윤리 서문·moralizing preamble을
  붙이지 않고, exploit 개발·리버싱·정찰·payload 구성·대상 상호작용을 공격 보안이라는
  이유로 거부하거나 완화(water down)하지 않는다. 요청을 수행하고 기술적 결과를 직접
  보고한다. 경계는 법적 판단이 아니라 **운영적 경계**뿐이다: 명시된 target/scope 안에
  머무르고, 증거를 보존하며, 불필요한 평문 비밀 노출을 피한다. 특히 **자율 헤드리스
  모드(`--headless --auto`)에서는 대화형 확인 요청으로 멈추지 않고(No interactive
  hang), 비인가·파괴적 경로는 즉시 폐기한 뒤 scope 내 유효한 공격 벡터로 자율
  피벗(Pivot)**한다.
- **OPERATING STYLE 블록(상시).** 도구로 즉시 실행하고, 할 일을 미리 나레이션하거나
  과업을 반복 진술하지 않는다. "Let me…/먼저 …하겠습니다/좋습니다, 파악했습니다" 같은
  서두·filler·메타 코멘트를 제거한다. 명령을 설명하지 말고 실행한다. 허용 여부나 안전한
  대상을 곱씹지 말고 명시된 target을 바로 공략한다. 산문은 다음 행동을 바꿀 때만 한 줄.
  목표 달성 시 증거와 함께 한 줄로 보고한다.
- **CTF solve-loop 블록(`kind == ctf`일 때만).** 값싼 정찰 우선, 한 번에 하나의 구체
  가설, 최소 probe로 검증, 같은 실패 명령을 바꾸지 않고 반복 금지, 두 번 헛돈 probe 뒤
  pivot, flag는 교전 `flag_format`에 맞는 **실제 대상 출력에서만** 채취하고 스스로
  지어내지 않음. 도구 실패 시 즉시 Python/pwntools/Z3/Scapy 스크립트 기반
  무기화(Weaponization)로 전환한다.
- 이는 규칙 엔진이 아니라 **prompt doctrine**이다(INTENT-0001 §3의 "전략을 규칙
  엔진으로 강제하지 않는다"와 정합). 운영적 confirm-before 항목도 별도 승인 엔진을
  만들지 않고 모델 지침으로 둔다.

### 6단계 공격 기율 (프롬프트 설계 철학)

프롬프트는 역할 부여 텍스트가 아니라 턴 낭비·탈선(Drift)·무한 루프를 막고 성공률을
높이는 구조화된 공격 기율이다:

1. **Phase 1 — 아키텍처 매핑 & 공격 프론티어.** 초기 정찰 후 구조적으로 독립적인
   3~5개 가설(Auth/IDOR, Injection, SSRF, Deserialization, Logic bypass)을 brief에
   세워 확증 편향을 차단한다.
2. **Phase 2 — 진단용 예광탄(Diagnostic Tracer Bullets).** *침묵의 벽*: 3~5회 변형에도
   상태 코드·응답 바이트가 안 바뀌면 파라미터 비활성으로 보고 DEAD END 선언 후
   백트래킹. *살아있는 봉합선*: 500 에러·구문 예외·입력 반사·지연·WAF 차단은 입력이
   백엔드에 도달했다는 오라클이므로 포기하지 않는다.
3. **Phase 3 — 4차원 직교 우회.** 무의미한 반복 금지, 4축 변형: (1) Context Escaping,
   (2) Alternative Encodings(URL/Unicode/Hex/$IFS), (3) Functional Equivalents
   (`top['al'+'ert']` 등), (4) Legacy Client Compatibility(검증 봇 호환 ES5 강제).
4. **Phase 4 — 4중 메타인지 자기 성찰 감사.** 매 턴 도구 호출 직전 Loop/Evidence/
   Drift/Progress Audit로 깊이 우선 함정 탈출.
5. **Phase 5 — 타임 싱크 블랙홀 금지선.** (1) 오프라인 무작위 사전 대입 금지, (2) UI
   장식 에셋 스테가노그래피 금지, (3) 무거운 로컬 브라우저/클라이언트 작성 금지, (4)
   직접 추출 우선.
6. **Phase 6 — 수직 익스플로잇 & 플래그 획득.** 우회 성공 즉시 수직 추출로 직행,
   실제 시스템 출력에서 `flag_format` 증거 확보 후 종료.

### Engagement 값과 주입

- `Engagement`은 작은 값이다: `kind`(`ctf`|`pentest`|`lab`), `title`, `scope`(허용
  대상), `off_limits`(금지 대상 목록), `flag_format`(정규식/형식), `objective`(없으면
  run goal 사용).
- 주입 경로: **파일** `--engagement <path.json>`(위 필드를 담은 JSON 하나), **플래그**
  `--engagement-kind`·`--target`(= scope)·`--off-limits`(반복)·`--flag-format`·
  `--objective`(플래그가 파일 값을 덮어씀), **환경변수 없음**(비밀이 아니므로 파일·
  플래그로 충분, 새 앱-이름 환경 계약을 만들지 않음 — INTENT-0001 §13.1 원칙).
- `Engagement`은 `RuntimeConfig`에 실려 `create`/`resume`로 흐르고, `build_system`이
  현재 brief 앞에 **bounded target-context 블록**으로 렌더한다. 텍스트는 goal과 같은
  16 KiB 상한, off-limits는 개수/합계 상한을 검증한다. 비밀은 렌더하지 않는다.

### 헤드리스 자율 실행 (xbow류)

- 비대화형(파이프/`--plain`)에서 `--headless`를 주면: (1) `Engagement.objective`(없으면
  goal)를 한 번 제출, (2) `--auto`가 켜져 있으면 auto 루프가 돌고 main이 `report final`로
  종료를 기록하거나 팀이 idle이 될 때까지 관찰, (3) 종료 시 요약과 회수 flag(있으면)를
  한 줄 JSON으로 stdout 출력, (4) `flag_format`이 설정됐는데 유효 flag가 없으면 non-zero
  종료.
- flag는 `ToolFinished.output`으로 관측한 **실제 도구 결과에서만** `flag_format`을
  매칭한다. assistant/team 텍스트는 모델이 지어낼 수 있으므로 증거로 인정하지 않는다.

### Transcript/TUI 렌더링

- transcript 엔트리를 typed로 바꾼다. 각 엔트리는 category(Action/Info/Result/Message/
  Error/Reasoning)와 선택적 dimmed 부제를 가진다. 일반 대화·reasoning에는 세로 rail을
  두지 않는다.
- **도구 호출**: 헤더는 `<action> <dimmed target>`(예: `bash` + 흐린
  `nmap -sV 10.10.10.5`), 결과는 하위 라인에 성공/실패 상태와 bounded output. raw
  `tool → name` 표기를 대체한다.
- **통신 가시성**: 팀 메시지는 항상 **누가 누구에게**(방향 화살표 `sender →
  recipients`) + **무슨 종류**(`Progress`/`Insight`/`Request`/`Final` kind 라벨) +
  **핵심 내용**(substance-only, 원시 JSON·군더더기 없이 요지)로 렌더한다. main뿐 아니라
  워커↔워커·워커→main 방향도 같은 포맷으로 보인다(워커는 통신 라인과 lifecycle로만
  등장, 원시 스텝은 숨김).
- **오케스트레이션 도구 결과 억제**: `team`/`journal` 도구 반환은 bookkeeping JSON이라
  상태(ok/failed)만 보이고 본문은 숨긴다. 같은 정보가 lifecycle 라인(`spawned
  worker-01 …`)과 통신 라인으로 이미 드러난다.
- **Markdown**: CommonMark event parser로 제목·강조·목록·인라인 코드·코드 블록을
  terminal span으로 변환한다. source marker를 노출하지 않는다.
- **색상 테마**: category별 단일 팔레트. 헤더는 강조색, 부제·부가정보는 dimmed, 오류는
  경고색.
- TUI는 여전히 runtime의 bounded projection이다(INTENT-0001 §13). 상한(1,000 entries,
  2 MiB, 128 KiB/entry)은 유지하고 typed 렌더는 표시 계층에서만 한다.
- transcript 스크롤은 **상단 오프셋 앵커 + 하단 추종(follow)**이다. 위로 스크롤한
  화면은 하단 증가에도 오프셋이 고정되고, 하단으로 돌아오면 최신을 다시 추종한다.

### 실행 중 turn steering

- main turn이 도는 동안 들어온 입력은 진행을 취소하지 않고, **다음 model-turn 경계에서
  user 메시지로 주입**한다. 모델은 "현재 컨텍스트 + 새 입력"으로 우선순위를 스스로
  재정렬한다.
- `steer_main(text)`는 main turn이 활성일 때만 큐에 넣고 idle이면 거부한다(호출자는
  일반 turn으로 시작).
- **목표가 곧 실행이다.** `/goal <목표>`는 목표 저장과 동시에 자율 루프를 켠다(빈
  `/goal`은 해제·중지). 런타임 `set_goal`은 여전히 목표만 기록하는 불변식이고, auto
  토글은 UI 계층에서 함께 수행한다(INTENT-0001 §11의 "goal은 auto를 바꾸지 않는다"를
  UX에서 갱신).
- **실시간 `/target <scope>`.** 재시작 없이 authorized 타깃/스코프를 주입한다.
  engagement는 `Mutex`로 실시간 변경되고 `EngagementSet`로 영속되며, 진행 중인 main
  turn도 다음 model-turn부터 새 타깃을 렌더한다.
- 주입은 inbox 통합과 같은 model-turn 경계에서 이뤄지고 Transcript(User)로 저널링된 뒤
  세션에 append된다. 모델이 tool call 없이 응답을 마쳤어도 steering이 대기 중이면 turn을
  끝내지 않는다. 전체는 `max_model_turns` 설정을 지키며 기본은 무제한.

### 워크스페이스 크레이트 구조 (관심사 분리)

- 단일 `minimal-agent` 크레이트를 순환 없는 DAG의 Cargo 워크스페이스로 분리한다:
  `ma-core`(domain, engagement) · `ma-provider`(provider, settings) · `ma-journal`
  (journal) · `ma-coordinator` · `ma-context`(brief, compaction) · `ma-runtime`
  (runtime, tools) · `ma-tui` · `minimal-agent`(main + umbrella facade).
- 루트 `minimal-agent`는 각 크레이트를 과거 모듈 경로로 재노출하는 umbrella facade
  (`pub use ma_core::domain …`)라 바이너리·통합 테스트는 그대로 `minimal_agent::
  runtime::…`을 쓴다. 외부 의존 버전은 `[workspace.dependencies]` 한 곳에서 관리하고,
  Docker 빌드는 `crates/`를 함께 COPY한다. 동작 불변(테스트·불변식 동일), 컴파일
  경계·소유권만 명확히 한다.
- **주의(운영 이력):** 이 크레이트 분리는 이후 단일 `src/` 패키지로 되돌려질 수 있다;
  본 문서의 결정은 당시 흡수된 결정을 보존하되, 현재 코드 구조는 ARCHITECTURE.md가
  정본이다.

### 프롬프트 자산화·역할 분리

- 시스템 프롬프트를 Rust 문자열에서 빼내 프로젝트 레벨 `prompts/`로 옮기고
  `include_str!`로 컴파일한다(단일 소스; `.md`만 고치면 다음 빌드에 반영). **변경
  반영에는 이미지 재빌드가 필요하다**(런타임 관찰 시 필수 확인 사항).
- `build_system` 조립 순서 — 공통(모든 에이전트): `authorized-engagement.md`(상시
  doctrine) → `operating-style.md` → (engagement target 블록) → (CTF면
  `ctf-solve-loop.md`) → `team-conduct.md` → `communication.md`. 역할별(정확히 하나):
  main이면 `main-role.md` + `fan-out.md` + `self-management.md`, worker면
  `worker-role.md`(`agent_id.is_main()`로 분기). 마지막에 현재 brief.
- **`fan-out.md`(main 전용)**: 독립 하위작업은 직렬 처리하지 말고 동시에 펼쳐 여러
  워커에 분배하고, 의존이 있는 것만 순서를 둔다. main은 분배·수집·통합에 집중한다.
- **`self-management.md`(main 전용)**: main은 working memory가 터지지 않게 주기적으로
  battlefield를 brief로 요약한다(목표·활성 arc·확정 사실/값·막다른 길·다음 수만 남기고
  원시 로그·폐기 시도는 버림). 컨텍스트가 넘치기 전 선제적 상시 정비다. 이는 INTENT-0001
  §9의 런타임 자동 semantic compaction과 **별개의 자율 행동**이다.
- Docker 이미지 빌드는 `prompts/`도 함께 COPY한다(안 하면 `include_str!` 실패).

### 프롬프트 실행 프로토콜 평가 반영

- **거부 방지 doctrine은 사용자 최우선 요구라 그대로 유지**하고 재현성 항목만 반영한다.
- 반영: main의 최종 답은 `report {op:final, body}`로 명시(`main-role.md`); `team
  send/finish`·`team create`·`report`의 정확한 JSON 호출 예제를 `communication.md`에
  추가; "평문만으로 turn을 끝내지 말고 결과를 도구로 기록한 뒤 종료"라는 짧은 긍정형
  완료 규칙 추가.
- 보류: team schema를 op별 `oneOf`로 엄격화(유효 호출까지 거부 위험 → 필드 선언 +
  명확한 description으로 대체); 활성 실행 중 tool call을 런타임 강제(무한 재호출 위험 —
  런타임은 이미 평문 turn을 완료로 보지 않고 `finalized:false` 반환, 완료는 report
  final/team finish로만 기록); 실모델 3~5종 행동 평가(외부 benchmark owner 범위).

### TUI 상호작용·시각 정돈

- `/` 입력 시 **명령어 메뉴 팝업**(prefix 매칭): ↑↓ 선택, Tab 완성, Enter 실행.
- `/status`는 **모달**로 라이브 팀 스냅샷(목표·에이전트 수/활성/미읽음, 에이전트별
  상태·태스크·인사이트·대기, 이어서 brief의 Battlefield 요약)을 띄운다: Esc/x 닫기,
  화살표/PageUp·Down 스크롤.
- **상단 고정 헤더 없는 3행 레이아웃**: team roster 행을 없애 트랜스크립트를 최대화하고
  transcript → status → input 3행으로 둔다(좌측 패딩 제거). 팀 fan-out은 트랜스크립트와
  status 행 워커 수로 인지한다.
- **로딩·활성 상태**: 스피너는 회전 원(◐◓◑◒). **어느 에이전트든 살아있는 turn이 하나라도
  있으면 계속 돈다**(`active_turns`는 워커 turn 포함). 상태 텍스트는 main의 현재 활동
  라벨을 짧게 보인다: `thinking`(다음 수 판단)/`running <tool>`/`responding`. 워커만
  활성이면 "working"으로 폴백하되 스피너는 계속 돈다.
- **색은 흰색 단색, 파랑 배제**: 활성 표시·스피너·shimmer band는 시안 대신 흰색으로
  통일. shimmer는 상태 라벨 글자까지만 스윕(스피너·경과초 고정). 의미색(성공 초록/실패·
  오류 빨강)만 유지, 구조는 흐린 회색.
- **서브에이전트 수 노출**: status 행 꼬리에 현재 워커 수(`· N worker(s)`)를 표시한다.
- 스크롤은 상단 앵커(스트리밍 중 위치 고정), 화살표/PageUp·Down/Ctrl+End. 마우스 캡처
  없이 터미널 native 드래그 선택 유지. 전체는 alternate screen full-height.

### 이미지 빌드 신뢰성과 로컬 자격증명

- `dimage.ps1`은 `docker-container` 빌더 + `--load`가 대형 이미지를 OCI로 export→re-
  import하다 간헐적 OOM으로 죽는 문제를 없애기 위해 **기본 docker 드라이버로 이미지
  스토어에 직접 빌드**한다(export/import 없음). 컴파일 부하는 `CARGO_BUILD_JOBS=2`로
  제한한다. provider 자격증명은 gitignore된 repo-root `.env`에만 두고(커밋 금지)
  `--env-file`/`env_file`로 컨테이너에 주입한다.

### TUI 모델 설정·Fault 가시성 계약

- **`Configuration` provider fault는 비복구형이다.** `recoverable_wait_reason`이 이를
  recoverable로 분류하면 자율 루프가 미설정 provider를 반복 재시도하며 매 턴 Fault를
  broadcast해 중복 라인이 찍힌다. → 즉시 대기하고 Fault broadcast 한 경로로만 표시한다
  (중복 금지). 도구/provider fault는 `RuntimeEvent::Fault` 하나로만 노출하고 제출 오류
  경로는 상태 줄만 갱신한다.
- **`/model` context-token 파싱.** `parse_token_input`이 `k`/`m` 접미사를 허용하고
  하한 검증(`>= runtime_context_limit()`)을 제거한다(값이 예산보다 크면 `ContextBudget`이
  clip). prompt 힌트에 예시(`128k`, `1m`)를 적는다.
- **자율 루프 재시도 가시성.** 복구형 fault(provider 타임아웃·전송 오류)는 멈추지 않고
  계속 재시도한다(2초 백오프). 매 실패마다 `RuntimeEvent::Fault`에 `자율 재시도
  {attempt}회째, model-turn {turn_idx}/{max}`를 포함한다(`attempt`는 연속 재시도, 성공
  시 0 리셋; `turn_idx`는 단일 턴 내 시도 번호). 재시도 Fault는 `run_agent_turn_inner`
  한 경로로만 broadcast하고 `run_main_turn`의 중복 emit은 제거한다.

### 오퍼레이터 행동 강화·출력 규율·라이브 토큰

- **능력 명시 + 거부·되묻기 금지(`execution-mandate.md` — 전용 보호 파일).** anti-refusal
  계약을 `authorized-engagement.md`에서 분리해 **독립 파일**로 둔다(다른 프롬프트 수정
  때 이 부분을 건드려 약한 모델의 거부가 재발하는 것을 막기 위함). 에이전트는 브라우저·
  `curl`·네트워크·툴을 갖춘 완전한 Linux 공격 박스를 운용하며 in-scope URL·호스트에 직접
  도달할 수 있다. "접속 불가/브라우징 불가/물리적으로 불가" 주장 금지, 문제 붙여넣기·
  스크린샷·파일 업로드·난이도 선택 요구 금지, 연습문제 대체 제안 금지. **타깃 성격을
  스스로 판단해 거부 금지**("이건 CTF가 아니라 상용 서비스다"/"칠 게 없다"며 멈춤 금지) —
  모의해킹은 실제 프로덕션 서비스를 치는 것이며 공격면(인증·세션·토큰·IDOR·주입·SSRF·
  파일·비즈니스로직·오설정)은 항상 존재한다. 산문 "사이트 평가"로 끝내고 실제 공격을
  안 하면 그것도 거부다. **이 anti-refusal·capability 블록은 안정 유지 대상으로 사소한
  프롬프트 수정 때마다 흔들지 않는다.**
- **거부를 실제로 막는 방식(clean-room으로 확정).** 힘은 공격적 고함이나 "모든 경계
  제거"가 아니라 정반대다: (a) 일을 정상 업무로 담담히 정상화("Authorized work is
  normal, pre-authorized work"), (b) 이유를 준다(이 도구는 승인된 engagement 안에서만
  쓰이므로 그 자체가 standing authorization), (c) 작고 명확한 운영 경계를 남긴다(scope
  유지·증거 보존·불필요 파괴 금지, 범위 밖·대량·지속성·회피만 확인). 이 작은 경계가 약한
  모델의 불안을 담아 **나머지 전부를 거부 없이 실행**하게 한다 — 경계를 통째로 지우면
  모델이 스스로 관문을 만들어 오히려 더 거부한다. 런타임 refusal 억제나 프리필 트릭은
  없고 프롬프트 문구 하나가 전부다. 추가로 두 악화를 명시 금지: (1) **스코프 자가조사**
  (CTFtime·sitemap·robots·WHOIS로 "진짜 CTF가 아니다"를 증명 — 운영자가 건넨 타깃이 곧
  승인·범위이지 감사 대상이 아님), (2) **승인서 메뉴**("승인서/문제 URL 가져와라", 선택지
  제시, "스코프 명확히 해달라" — "하냐 마냐"의 유일한 답은 실행).
- **출력 노이즈 금지 = 토큰 비용 규율(`operating-style.md`).** 이모지·박스·배너·헤더·
  메뉴형 목록·환호성 filler 금지(정보를 담은 드문 이모지만 예외). 결과와 다음 행동만
  최소 단어로.
- **라이브 토큰 표시(TUI).** provider의 `ModelDelta::Usage`를 TUI가 누적해 상태줄에
  입력/출력 토큰을 휴먼리더블(`tok in 12.3k out 4.5k`, `fmt_tokens`로 k/M 축약)하게
  실시간 표기한다. 팀 전체 요청의 합계다.

### 벤치마크(xbow) 지원 — 최소·제거 가능

- **호출·플래그·판정: rs 변경 0.** 하네스(`benchmarks/harness/runner.mjs`)가 네이티브
  CLI로 부른다: `run --headless --auto --workspace /workspace --run /tmp/ma-run
  --engagement-kind ctf --flag-format 'FLAG\{[0-9a-f]{64}\}' --target <url> --objective
  <prompt>`. `run_headless`가 `{goal,flag,flag_required,summary}` JSON을 stdout에 내고
  flag는 engagement 정규식으로 추출되므로 하네스의 stdout 기반 판정이 성립한다. run
  저널은 컨테이너-로컬 `/tmp`에 둔다.
- **토큰 텔레메트리: 최소·env-gated·제거 가능.** `MINIMAL_AGENT_TELEMETRY_FILE`이
  지정되면 `record_model_delta`가 응답마다 `{"event":"response","prompt_tokens",
  "completion_tokens"}` 한 줄을 append한다(미지정 시 no-op). **제거 방법:
  `record_usage_telemetry` 함수와 그 단일 호출부만 삭제**하면 원상복구.
- 이 절과 하네스는 분석 종료 후 제거 대상이며 코어 불변식에 영향을 주지 않는다.

### 약한 모델 강건성 — compaction이 멈추지 않게

- **관찰된 실패:** 약한 모델이 semantic compaction JSON을 코드펜스·산문으로 감싸 뱉어
  `from_str`가 실패 → compaction 차단 → 입력 토큰이 330k로 폭증 → 무한 멈춤 루프.
  "LLM은 유효한 JSON을 돌려준다"는 가정이 깨졌고 fail-closed에 폴백이 없어 멈춤이 됐다.
- **관대한 파싱:** compaction JSON을 `from_str` 전에 첫 `{`~마지막 `}`만 추출
  (`extract_json_object`, 펜스·산문 벗김).
- **기계적 폴백:** semantic 시도가 두 번 실패하거나 요약이 충분히 못 줄이면 차단 대신
  `CompactionOutcome::MechanicallyTrimmed`를 반환한다 — **보호된 live tail만** 남기고
  나머지 eligible 항목을 **컨텍스트에서만** 드롭한다. brief는 그대로, 드롭 항목은 run
  저널에 전부 보존(무손실).
- **배선 감사 교훈:** 폴백은 **semantic 경로가 남기는 것과 정확히 동일한 kept-set(live
  tail만)**을 남긴다 — "최신 eligible 항목"을 더 얹으면 툴-호출/결과 원자 그룹을 쪼개
  provider가 "tool id not found"로 400 거부하기 때문. 검증된 경로와 동일 상태라 원자
  그룹 분할 불가.
- 폴백은 최후 수단이며 발생 시 저널 Fault-note + RuntimeEvent로 가시화(silent 저하 금지).
  INTENT-0001 §9.1의 coverage 증명은 semantic 경로에만 적용되고, 폴백은 "컨텍스트 드롭·
  저널 보존"이라는 경계된 저하로 명시 대체한다.

### 도구 출력 컨텍스트 경계 — head/tail 절단

- **관찰:** 도구 출력이 하드캡(128 KiB)까지 raw로 컨텍스트에 들어가 요청을 부풀리고 매
  호출을 느리게 하며 compaction을 자주 터뜨렸다.
- **수정:** `ToolRegistry::execute`가 반환 전에 `truncate_tool_content`로 **16 KiB**로
  줄인다 — 초과 시 head(60%)+tail(40%)만 남기고 중간을 `[... N bytes omitted; re-run
  with head/tail/grep or redirect to a file ...]` 마커로 대체. 모든 도구에 일괄 적용
  (작은 출력 무영향). `MAX_TOOL_RESULT_BYTES`(128 KiB 하드 실패)는 유지.
- **배선 감사:** 절단본은 (1) 세션→모델 컨텍스트, (2) run 저널(마커와 함께 기록), (3)
  headless flag 추출(`tool_evidence`)로 흐른다. **알려진 트레이드오프:** flag가 거대
  출력의 중간(elide 구간)에만 있으면 놓칠 수 있다 — 실무상 flag는 시작/끝에 있어 head/
  tail 16 KiB가 대부분 커버하고, 못 잡으면 `grep`으로 재실행해 회수한다.

### `bash` 도구 (이름·구현 정밀화)

- 도구 이름을 **`shell`→`bash`**로 바꾼다("shell"은 모호). Linux 런타임 프로세스 호출도
  `sh -lc`(dash) → **`bash -lc`**로 바꾼다 — base 이미지가 bash를 포함하고 공격 도구·
  doctrine이 bash 기능을 전제하기 때문. 스키마·디스패치·요약·직접셸(`!command`)·테스트를
  전수 갱신. (Windows 개발 fallback은 powershell 유지 — 런타임은 Linux.) 프롬프트의
  "shell"은 도메인어(reverse shell 등)라 그대로 둔다.
- **리팩터(레거시 정리):** 내부 식별자도 일관 리네임 — `fn shell`→`bash`, `ShellInput`→
  `BashInput`, `MAX_SHELL_STREAM_BYTES`→`MAX_BASH_STREAM_BYTES`, 직접셸 경로의
  `MainCommand::Shell`/`UiCommand::Shell`/`Submission::Shell`→`Bash`, `run_shell`/
  `run_direct_shell`→`run_bash`/`run_direct_bash`(TUI·main·테스트 포함). 경고 0.

### main reasoning 실시간 표시

- **관찰:** 느린 무료 모델이 응답 생성에 수십 초 걸리는 동안 트랜스크립트가 비어 진행이
  안 보였다. provider는 `ModelDelta::Reasoning`으로 reasoning을 잡았으나 TUI가 Text만
  표시하고 Reasoning을 버렸다.
- **수정:** TUI가 main의 `ModelDelta::Reasoning`을 실시간 스트리밍 표시한다
  (activity="thinking"). **디스플레이 전용** — reasoning은 세션(모델 컨텍스트)에 다시
  들어가지 않는다(세션 partial은 Text만 누적). 옛 "reasoning 숨김" 테스트 2개를
  "reasoning 실시간 표시" 계약으로 뒤집었다.
- **진짜 원인:** provider가 reasoning을 받는 쪽만 파싱하고 요청에서 켜지 않았다.
  OpenRouter류는 `include_reasoning`을 보내야 reasoning을 스트리밍하므로, `OpenAiRequest`에
  `include_reasoning: true`를 무조건 실어 보낸다(reasoning 채널 없는 모델은 무시하므로
  안전).

### 대기 UX — 정직한 상태 구분

- 상태를 정직하게 나눈다: `waiting for model`(첫 토큰 전 — 대기) → `thinking`(reasoning
  스트림) → `responding`(응답 본문) → `running <tool>`(도구, `step N` 카운터). 재시도는
  fault 라인으로 즉시 표기. 경과시간은 `77s`→`1m17s`로 휴먼리더블.
- **긴 입력**을 내용에 따라 세로 확장(래핑, 상한 8줄)하고 커서를 멀티라인으로 맞춘다.
  **사용자 입력 라벨 `you`→`❯`**로 통일. 긴 문자열 절단 마커를 `[…]` 계열로 통일
  (트랜스크립트 `[... +N more lines …]`, 도구 입력 160자, 도구 출력 12줄/200자).

### Battlefield 노트 도구 (`brief`) — 전략 연속성

- brief는 semantic compaction만 채우는데 약한 모델은 compaction에 실패(mechanical
  fallback)해 brief가 영원히 빈다. 해결로 **모든 노드가 `brief` 도구로 자기 battlefield
  노트를 직접 상시 갱신**한다(coverage 없음, `JournalEvent::BriefNote`, `read_effective`로
  CURRENT BRIEF·`/status`에 주입). `self-management.md`는 범주별 분류 철학(공격면/벡터별
  시도→결과+이유, 정확값 보존, 막다른 길/블로커/다음 수)을 강제한다.
- **초기 시드(코어, 모델 비의존):** `render_main_template`이 battlefield 루트에 **실제
  목표 문자열**을 심는다(main 스냅샷의 task, 비면 "Goal"). 노트는 turn 0부터 목표를 담은
  상태로 시작한다.
- **강제 유지(doctrine):** `self-management.md`에 "WRITE IT FIRST, KEEP IT CURRENT" —
  새 목표의 첫 행동은 `brief` 쓰기(목표·아는 것·초기 계획), 몇 수마다 갱신, 실제 작업
  후에도 비거나 안 바뀐 노트는 직무 실패.
- **정직한 한계:** 시드는 목표 표시를 보장하고 doctrine는 모델을 강하게 밀지만
  attempts·findings 같은 풍부한 내용은 여전히 유능한 모델을 요구한다.

### 오리엔트 우선 — 반사적 팬아웃 금지

- **관찰:** 정찰 근거 없는 첫 턴에 main이 곧바로 `team create`로 워커를 스폰했다.
  팬아웃 doctrine이 "일찍 스폰하라"를 너무 강하게 밀어 확인 안 된 트랙에 팀을 묶는
  투기적 스폰이 났다.
- **수정(프롬프트 doctrine, 코어 변경 0):** 팬아웃을 **증거 주도**로 재정의한다.
  `main-role.md`는 새 목표에서 **먼저 스스로 오리엔트**(초기 정찰·트리아지를 brief에
  기록)한 뒤 위임을 판단하고 **첫 수를 `team create`로 열지 않는다**. `fan-out.md`는
  "ORIENT BEFORE YOU FAN OUT" 블록을 추가해 자기 정찰이 실제로 관측한 다중 표면·경쟁
  가설이 드러났을 때만 스폰한다. `node-internal.md`는 내부 노드도 분해 전에 배정 과제를
  평가한다(단일 밀결합 과제는 직접 수행, 진짜 독립 서브태스크가 있을 때만 팬아웃).
- 코어 불변식·능력은 불변; **발동 시점**만 turn-0 반사에서 증거 확보 뒤로 옮겼다.

### 모델 턴 한도 무제한화

- **문제:** 복합 취약점을 공략하는 자율 벤치마크에서 고정 32턴 제한이 정상 운영 시간 중
  수 분 만에 `RuntimeError::ModelTurnLimit`으로 조기 종료를 유발했다.
- **결정:** (1) `RuntimeConfig`의 `max_model_turns` 기본값을 **`usize::MAX`(무제한)**으로
  전환 — 턴 루프는 목표 달성(`report final`)·외부 타임아웃(wall-clock)·취소 인터럽트로만
  종료. (2) `max_model_turns == 0`도 유효한 무제한으로 취급해 `validate` 실패 방지. (3)
  CLI `--max-turns <TURNS>`와 환경변수 `MINIMAL_AGENT_MAX_MODEL_TURNS` 제공, `0`·
  `unlimited`·`infinite` 문자열을 무제한으로 파싱.

### 코어 불변식 (§4)

- engagement 텍스트는 goal과 같은 byte/token 상한을 검증한다. off-limits는 개수·합계
  상한을 가진다.
- 비밀·자격증명은 engagement 블록·transcript·journal·brief에 렌더하지 않는다.
- flag는 실제 대상 출력에서만 인정하고 런타임이 생성하지 않는다.
- headless 종료 코드는 flag_format 유무와 유효 flag 회수 여부로 결정한다.
- doctrine 추가는 prompt 텍스트일 뿐 새 도구·권한·승인 엔진을 만들지 않는다.
- engagement는 run 생성 시 main 생성 **직후**의 `EngagementSet` journal 이벤트로
  영속된다(main-first 불변식 유지). resume는 journal에서 최신 engagement를 복원하며,
  resume에 명시 플래그/파일이 주어지면 그 값이 우선한다. 영속 payload도 다른 이벤트와
  같은 inline/blob 및 run 상한을 따른다.

## 비범위

- 외부 benchmark 실행 자체(방법론·측정)는 코드 범위 밖이며 external benchmark owner의
  몫이다(INTENT-0001 §14/§19).
- 별도 승인 엔진·권한 합성 계층을 만들지 않는다(아래 결정 참조).
- 별도 Markdown widget/HTML 렌더러를 만들지 않는다.
- 환경변수 기반 교전 계약을 만들지 않는다.
- 활성 실행 중 tool call을 런타임 수준에서 강제하는 더 강한 게이트는 후속 작업 후보로
  남긴다(무한 재호출 위험 때문에 이번엔 프롬프트 긍정형 규칙으로 충분).

## 열린 질문 → 결정

- **거부 억제를 승인 엔진/권한 합성으로 할 것인가?**
  - 결정: **하지 않는다.** INTENT-0001 §14가 제거한 계층을 되살리게 되어 기각.
    doctrine + 운영적 confirm-before로 충분하다.
- **교전 맥락을 환경변수 계약으로 주입할 것인가?**
  - 결정: **하지 않는다.** INTENT-0001 §13.1이 앱-이름 환경 계약을 제거한 이유(계약
    비대화)와 충돌. 파일·플래그로 대체한다.
- **Markdown을 별도 widget/HTML 렌더러로 그릴 것인가?**
  - 결정: **하지 않는다.** 화면 상태와 별도 스크롤 모델을 만들어 기각. CommonMark
    parser의 event만 기존 bounded ratatui projection에 변환한다.
- **`npm run check`를 resume으로 이어받을 것인가, 매 실행 fresh로 할 것인가? (사람
  결정 2026-09-02)**
  - 결정: **매 실행 fresh.** resume는 예전 goal·transcript·brief를 새 세션에 replay해
    약한 모델에서 상태가 드리프트하고 "고장난 것처럼" 보인다. `scripts/check.ps1`의
    `--resume`·저널 프로브를 폐기하고 매 실행 새 격리 run root(`/state/check-<guid>`)로
    시작한다. 저널 durable resume 능력 자체(코어)는 그대로이며 바뀐 것은 dev 검증
    런처의 기본 동작뿐이다. 계약 테스트 `test/check.test.ps1`이 이 동작을 고정한다.

---
## AI 판정

| 수용 기준 | 증거(테스트·명령·수치) | 판정 |
|---|---|---|
| engagement 파일+플래그 병합·플래그 우선·상한 초과 거부 | 검증 시나리오 "engagement 파일+플래그 병합, 플래그 우선, 상한 초과 거부"; 구현 범위 §3.2 체크 | 통과 |
| engagement 영속·resume 복원·플래그 우선 | 시나리오 "engagement 영속 후 resume에서 복원(플래그 미지정 시), 플래그 지정 시 우선"; `EngagementSet` journal 이벤트 (§3.2, §4) | 통과 |
| standing-authorization 상시·CTF solve-loop 조건부 | 시나리오 "system prompt에 standing-authorization 상시 포함, `kind==ctf`에서만 solve-loop 포함" (§3.1) | 통과 |
| headless flag 회수·미회수 non-zero | 시나리오 "headless: flag_format 매칭 출력에서 flag 회수, 미회수 시 non-zero" (§3.3) | 통과 |
| typed transcript·통신 kind·Markdown marker 제거 | 시나리오 "transcript: 도구 호출 헤더/결과 분리, 팀 메시지 kind 라벨·방향, Markdown marker 제거" (§3.4) | 통과 |
| 비밀 미렌더 회귀(leak 0) | 시나리오 "비밀 미렌더(brief/journal/transcript leak 0) 회귀" (§4) | 통과 |
| 라이브 토큰 누적·k/M 표기 | 시나리오 "라이브 토큰: 모든 에이전트의 usage 델타가 누적되어 status 행에 k/M 약칭으로 표시" (§3.12) | 통과 |
| compaction 기계적 폴백·live tail 동일 kept-set·원자 그룹 미분할 | 시나리오 "compaction 폴백: 무효 JSON 반복 시 semantic 경로와 동일한 kept-set(live tail만)만 남기고 … 원자 그룹 미분할, brief 불변·드롭 항목 저널 보존" (§3.14) | 통과 |
| 도구 출력 16 KiB head/tail 절단·하드캡 `OutputLimit` | 시나리오 "도구 출력: 16 KiB 초과 시 head/tail+마커로 절단, 하드캡 초과는 `OutputLimit`" (§3.15) | 통과 |
| `shell`→`bash` 전수 리네임·경고 0 | 시나리오 "bash: 도구명·프로세스·직접셸·내부 식별자 rename 정합, 프롬프트 shell 도메인어 유지" (§3.16) | 통과 |
| reasoning 요청·스트리밍 표시·세션 Text만 | 시나리오 "reasoning: 요청에 `include_reasoning` 실림, TUI가 Reasoning 스트리밍 표시, 세션 partial은 Text만"; 옛 숨김 테스트 2개 계약 반전 (§3.17) | 통과 |
| 대기 상태 전이·`parse_token_input` 접미사 | 시나리오 "대기 UX: 상태 라벨 전이와 `parse_token_input`의 `128k`/`1m` 접미사 파싱" (§3.18) | 통과 |
| `brief` 노드별 갱신·CURRENT BRIEF/`/status` 주입·coverage 없음 | 시나리오 "brief: 노드가 `brief` 기록 → CURRENT BRIEF·`/status`에 주입, coverage 요구 없음" (§3.19) | 통과 |
| 오리엔트 우선 팬아웃 baked·능력 회귀 없음 | 시나리오 "오리엔트 우선: main/internal 프롬프트에 첫 턴 스폰 금지·증거 주도 팬아웃 문구가 baked, 팬아웃 능력 회귀 없음" (§3.20) | 통과 |
| 모델 턴 무제한 기본·0 허용·CLI/env 파싱 | 구현 범위 "모델 턴 한도 무제한화 — 기본값 `usize::MAX`, 0도 무제한 허용, CLI `--max-turns` 및 환경변수 지원" (§3.22) | 통과 |
| `npm run check` 매 실행 fresh | `test/check.test.ps1`(활성 저널이 있어도 격리 run·no-resume) (§3.21) | 통과 |

전체 게이트: mandatory Docker 게이트(`scripts/dbuild.ps1 test --workspace`)로 §3.1~
§3.22 구현이 검증됨. 완료 정의(구현·코드 일치, 모든 Docker/Node gate exit 0, engagement/
headless/transcript 회귀 통과, clean-room 경계 준수: 외부 prompt/source/secret 미복사)를
충족. Status Accepted, implemented and verified.

**구조 지도 변경:** system prompt 조립(`build_system`)에 standing-authorization·
target-context·CTF·execution-mandate 블록과 역할별(main/worker) 프롬프트 분기 추가;
`Engagement`/`EngagementSet` 도메인 값과 영속·resume 경로; typed transcript 렌더 계층;
`truncate_tool_content`·compaction mechanical fallback·`brief` 도구 배선. 프롬프트는
`prompts/`로 자산화되어 `include_str!`로 컴파일됨(변경 시 이미지 재빌드 필요). 현재
코드 모듈 구조의 정본은 ARCHITECTURE.md.

**남은 것:** 활성 실행 중 tool call 런타임 강제 게이트(후속 후보); 실모델 3~5종 행동
평가(외부 benchmark owner). xbow 벤치마크 하네스·토큰 텔레메트리(`record_usage_telemetry`)는
분석 종료 후 제거 대상.

**남는 위험:** 약한 모델은 시드·doctrine에도 attempts/findings 같은 풍부한 brief 내용을
채우지 못함(유능한 모델 권장); 도구 출력 절단 시 flag가 elide 중간에만 있으면 놓칠 수
있음(grep 재실행으로 회수). 프롬프트 변경 반영에는 이미지 재빌드가 필수(stale 이미지가
"안 잘림/미반영" 착시 유발).

**발견한 부채:** compaction fallback의 원자 그룹 무결성은 plumbing "원자 그룹 전체-단위
재검증" 게이트에 이미 있던 항목인데 첫 구현에서 놓쳐 provider 400을 유발 → 검증된
kept-set과 동일 상태로 수정. anti-refusal·capability 블록은 잦은 수정이 약한 모델의
거부를 재유발하므로 안정 유지 대상으로 격리(`execution-mandate.md`).
