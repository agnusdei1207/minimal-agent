# INTENT-0004 · Bounded Three-Depth Hierarchical Orchestration and Exact-Value Propagation

- 상태: 진행 중
- 작성 2026-09-01 / 갱신 2026-09-08

## 왜

INTENT-0001이 세운 평평한 스타 토폴로지(main 하나 + worker 최대 9, depth 1)는 단순하고
통제하기 쉬웠지만, 여러 층으로 갈라지는 실제 교전에서 두 가지 한계가 드러났다.

- **중첩 병렬성의 부재.** depth 1 worker가 "웹 서비스 전체 정찰"처럼 큰 하위 과업을 받으면,
  그 아래에 다시 worker를 두어 엔드포인트별·가설별로 병렬화하지 못하고 순차로 처리하는 병목이
  생긴다.
- **정보 왜곡(전화 게임).** 트리가 깊어질수록 중간 노드가 하위 노드의 기술적 세부를 모호하게
  요약("오프셋을 찾았습니다")하여, 위쪽 리더가 올바른 익스플로잇 판단을 내릴 근거를 잃는다.

이 인텐트가 되면, main 하나로는 감당 못 하던 다층 과업을 경계 지어진 트리로 병렬화하면서도,
공격에 필수적인 정밀 값이 위로 올라가는 동안 손상되지 않는다. 이는 INTENT-0001의 토폴로지와
통신 규약을 개정한다(스타 → 경계 지어진 3단 트리, 전역 브로드캐스트 → 이웃 한정).

## 무엇

- main이 자식을 만들고, 그 자식이 다시 손자를 만들어 최대 3단(main → child → grandchild)까지
  트리로 병렬 작업한다. 손자는 더 깊은 하위를 만들 수 없다.
- 노드는 자기 위치(깊이·부모 유무·자식 유무)에 따라 다르게 행동한다. 자식을 둔 내부 노드는
  위임·취합·요약을 맡고 직접 도구 실행을 삼가며, 자식 없는 리프 노드만 실제 도구를 실행한다.
- 리프가 발견한 정확한 식별 값이 위로 올라가는 동안 임의로 지워지거나 추상화되지 않고 온전히
  보존되어 리더에게 도달한다.
- 노드는 직계 이웃(부모·자식·형제)하고만 대화한다. 조부모-손자 건너뛰기나 사촌 간 직통은 없다.
- 노드가 죽으면 그 아래 서브트리 전체가 함께 회수되어, 주인 없는 손자가 남지 않는다.

## 수용 기준

- [x] depth 2(리프) 노드가 team create를 호출하면 `MaxDepthReached` 도메인 에러로 거부된다.
- [x] 노드 생성 시 depth = 부모 depth + 1로 설정되고, 전체 활성 에이전트(main 포함)가 10을
      넘지 않는다.
- [x] send의 audience가 비이웃(조부모·손자·사촌)이면 `NonNeighborAudience`로 거부되고,
      직계 부모·자식·형제만 저널 append와 Inbox 투영이 성립한다.
- [x] main 자동 audience 포함이 폐기되고(`MissingMainAudience` 제거), main 전용 호출은
      비-main 발신 시 `CallerNotMain`으로 거부된다.
- [x] 리프의 team finish가 빈 audience로 사라지지 않고 직속 부모로 버블업된다.
- [x] 내부 노드를 terminal 처리하면 서브트리 전체가 Stopped로, recall하면 서브트리 전체가
      Recalling+취소로 캐스케이드된다.
- [x] worker가 회복 불가 fault로 비정상 종료하면 `Faulted` mark_terminal 경로가 그 서브트리를
      회수한다(죽음 감지).
- [x] build_system이 노드 위치를 담은 POSITION 블록과 `team-tree.md`를 항상 주입하고, 비-main
      노드에 자식이 있으면 `node-internal.md`, 리프이면 `node-leaf.md`를 선택 삽입한다.

## 제약

- **최대 깊이 불변식: `depth <= 2`.** `AgentDepth`는 0..=2 경계 u8 newtype(`MAIN`/`is_main`/
  `can_spawn`/`child`)이며, depth 2의 하위 생성은 런타임에서 `MaxDepthReached`로 거부한다.
- **팀 상한 불변식.** 계층과 무관하게 전체 non-terminal 에이전트 수는 main 포함 최대 10명으로
  유지한다(INTENT-0001 §5 정합). 별도의 전역 폭발 상한은 새로 심지 않는다 — 총 활성 permit
  상한 하나로 충분하다.
- **직계 통신 불변식.** 발신자와 수신자는 (1) 부모-자식이거나 (2) 부모가 같은 형제여야만
  통신이 성립한다. 조부모↔손자(skip-level)와 사촌 간(cross-branch) 직통은 런타임에서 거부한다.
  하위 노드의 통찰·최종 요약은 직속 부모로 전달되고, 부모가 합성하여 main으로 올리는 계층적
  집계를 따른다.
- **단일 저널 원장 불변식.** 모든 계층의 메시지와 이벤트는 여전히 단 하나의 Run Journal에
  순차 기록된다(INTENT-0001 정합).
- **원시 값 보존(Anti-Chinese-Whispers).** 리프는 추상 산문("취약점 발견함")을 금지하고 정확한
  식별 값(엔드포인트, 포트, 메모리 오프셋 `0x...`, leak 바이트, 자격증명 `user:pass`, 토큰,
  플래그 문자열)을 반드시 포함해 보고한다. 중간 노드는 하위가 보고한 원시 값과 구체적 기술 팩트를
  임의로 삭제·추상화하지 않고 온전히 보존하여 상위로 전파한다(Exact-Value Retaining Synthesis).
- **위치 기반 역할 분기.** 내부 노드(자식 있음)는 과업 분할·자식 지휘·결과 취합·전장 요약을 맡고
  직접 도구 실행을 삼가 자기 문맥이 대용량 터미널 출력으로 오염되는 것을 막는다. 리프 노드는
  실제 환경과 상호작용하는 유일한 도구 실행 주체이며, 즉시 도구를 실행하고 기술적 결과를 직속
  부모에게 직통 보고한다.
- **죽음 감지 + 캐스케이드 회수.** 의도적 recall·terminal은 서브트리 전체를 함께 종료한다.
  중간·자식 노드의 비정상 종료를 감지해 고아가 된 서브트리를 회수한다(수명 종료 guard가 기존
  mark_terminal 경로로 하위 트리·슬롯을 회수, INTENT-0001 kill-on-drop 정합).
- **에이전트 자산 언어 = 영어.** `prompts/*.md`, `prompts/skills/*.md`는 영어로 저술한다.

## 비범위

- 무한·가변 깊이 재귀 트리. 깊이는 정확히 3단(0·1·2)으로 못 박으며, 동적 상한 튜닝은 하지 않는다.
- 전역 폭발 상한(노드당·서브트리당 별도 permit). 총 활성 ≤10 하나로 갈음한다.
- 프로세스 abort·강제 종료(SIGKILL) 상황의 즉시 회수. destructor가 실행되지 않는 이 경로는
  죽음 감지 보장 밖이며 durable resume가 담당한다.
- 재시작 시 깊은 손자 구조의 무손실 재구성(아래 A-light 결정 참조).

## 열린 질문 → 결정

- Q: 재시작 복구를 순수 방식 A(루트만 재위임, 무거운 복구 머신 없음)로 하면 INTENT-0001의
  "워커가 durable inbox·session·context와 함께 resume" 불변식과 정면 충돌한다. 어느 쪽을
  버리는가? — 선택지: (A) durable resume 삭제, (B) 깊은 트리 재구성, (A-light) 둘의 절충.
  - 결정: **A-light.** 재시작은 INTENT-0001대로 워커를 durable resume하되, 깊은 손자 구조는
    재구성하지 않는다 — recover가 비-main 노드를 main의 직속 자식으로 flatten한다. "무거운 복구
    머신 금지"라는 방식 A의 취지를 만족하면서 멀쩡한 durable resume는 버리지 않는다. 코어는 별도
    변경 없이 이 동작을 만족한다.
- Q: 자원 상한을 3단 트리에 맞춰 새로 심을 것인가(3단×노드10=최대 111)? — 선택지: 전역 폭발 상한
  신설 vs 기존 총 활성 ≤10 유지.
  - 결정: **기존 총 활성 ≤10 유지.** 이론상 111도 과하지 않으나 현행 전역 permit 상한 하나로
    폭주를 원천 차단하므로 별도 상한을 새로 심지 않는다.
- Q: depth 초과 생성 거부 에러 이름을 무엇으로 하는가? — 선택지: `InvalidDepth` vs `MaxDepthReached`.
  - 결정: **`MaxDepthReached`.** `AgentDepth`는 0..=2 경계 newtype으로 구현하고 depth 2 하위
    생성을 이 이름으로 거부한다.

---

## AI 판정

| 수용 기준 | 증거 | 판정 |
| --- | --- | --- |
| depth 2 생성이 `MaxDepthReached`로 거부 | `AgentDepth` 0..=2 u8 newtype(`can_spawn`/`child`); 생성 시 depth<2만 허용, 리프 생성 시 `MaxDepthReached`. coordinator 계약 테스트. | 통과 |
| depth=부모+1, 활성 ≤10 | `AgentRecord`/`AgentSnapshot`의 `parent` 추적 + 생성 시 depth=부모+1; 총 활성 permit ≤10 유지. | 통과 |
| 비이웃 send 거부, 이웃만 성립 | `send`의 직계-이웃(부모∨자식∨형제) 검증, `NonNeighborAudience`. | 통과 |
| main 자동 audience 폐기, `CallerNotMain` | main 자동포함·`MissingMainAudience` 폐기; `require_main` 정정으로 `CallerNotMain`. | 통과 |
| team finish 부모로 버블업 | 런타임 `team finish`가 빈 audience 대신 부모로 버블업. 계약 테스트 갱신. | 통과 |
| 내부 노드 종료/recall 서브트리 캐스케이드 | `mark_terminal`이 서브트리를 Stopped로, `recall`이 Recalling+취소로 캐스케이드; `collect_descendants`(parent BFS)+`cascade_subtree`. 테스트 `terminating_or_recalling_an_internal_node_cascades_to_its_whole_subtree`, `recalling_an_internal_node_cascades_recall_to_its_subtree`. | 통과 |
| 비정상 fault 서브트리 회수(죽음 감지) | worker_driver가 회복 불가 fault를 `Faulted`, recall을 `Stopped`로 mark_terminal → 서브트리 회수; 수명 종료 guard가 unwind panic도 mark_terminal 경로로 회수. | 통과 (abort·강제 종료는 보장 밖, durable resume 대상) |
| POSITION 블록 + team-tree/node-internal/node-leaf 주입 | `build_system`이 POSITION 블록(depth·리프/내부·부모·자식·형제) + `team-tree.md` 상시 주입, 비-main에 `node-internal.md`(자식 있음)/`node-leaf.md`(리프) 선택 삽입. | 통과 |

전체 워크스페이스 `cargo test`(Docker 게이트) green: 172 passed / 0 failed(단일 스레드).

**구조 지도 변경:** 도메인에 `AgentDepth`(0..=2 newtype)·`AgentRecord.parent`; coordinator에
`parent_id` 기반 이웃 통신 검증과 `cascade_subtree`/`collect_descendants`; runtime `build_system`의
위치 기반 프롬프트 분기와 `team-tree.md`/`node-internal.md`/`node-leaf.md` 자산. ARCHITECTURE.md의
토폴로지·통신·프롬프트 항목을 이 인텐트 번호로 갱신할 것.

**남은 것:** 코어 증분(A·B)은 완료·검증되었으나 완료 승격 조건인 실 교전 E2E 스모크(리버스셸 등,
INTENT-0003의 상호작용 셸 계약과 공유)가 아직 green으로 실측되지 않았다. 이것이 통과하면 상태를
완료로 올린다. 그 전까지 진행 중으로 둔다.

**남는 위험:** 프로세스 abort·SIGKILL은 destructor 미실행으로 즉시 회수 보장 밖이며 durable
resume에 의존한다. A-light 복구는 재시작 후 손자 구조를 flatten하므로 깊은 트리의 원형은 복원되지
않는다(의도된 트레이드오프).

**발견한 부채:** 캐스케이드 계약 테스트 두 개는 첫 구현에서 누락되어 코드리뷰 감사로 사후 고정되었다.
유사 불변식(이웃 통신·깊이 상한)도 회귀 방지 테스트가 계약에 고정돼 있는지 주기적으로 확인할 것.
