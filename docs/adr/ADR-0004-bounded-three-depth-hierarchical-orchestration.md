# ADR-0004: Bounded Three-Depth Hierarchical Orchestration and Exact-Value Propagation

- Status: Implemented (후속 코어 수정 및 검증 상태는 docs/benchmark/project-audit-2026-09-05.md 참조)
- Created: 2026-09-01 +09:00
- Version target: 0.111.0 (ADR-0002/0003과 함께 릴리스). Docker 게이트가 초록이고 사용자가
  요청하기 전까지 crate 버전은 0.110.0으로 유지한다(ADR-0002 버전 홀드 규율).
- Repository: `agnusdei1207/minimal-agent`
- Relation: ADR-0001(코어 토폴로지)과 ADR-0002(교전 주입), ADR-0003(도구 모델)을 확장한다. ADR-0001의 단일 Journal 원장, 단일 소유자 Brief, Semantic Compaction 및 활성 팀 최대 10명 상한은 엄격히 유지하되, **토폴로지 깊이를 최대 3단계(Depth 0~2)로 확장하고 노드 역할 분리 및 직계 통신 불변식**을 규정한다.

---

## 1. 한 문장 결정

`minimal-agent`의 팀 토폴로지를 **최대 3-Depth(Root $\to$ Intermediate Lead $\to$ Leaf Worker) 계층 트리**로 한정 확장하고, 자식 유무에 따라 **내부 노드(위임·취합)와 리프 노드(도구 실행)의 프롬프트 역할을 분기**하며, 정보 왜곡(전화 게임)을 막기 위해 **핵심 원시 값(Exact Values) 보존 원칙**과 **직계 부모-자식 및 직계 형제 간 통신 한정 불변식**을 강제한다.

---

## 2. 배경과 문제

ADR-0001의 평평한 스타 구조(Depth 1, Main 1 + Worker 최대 9)는 단순성과 통제성이 뛰어나나, 복합적이고 다층적인 모의해킹/CTF 과업(예: 다중 서브넷 정찰과 특정 서비스 심층 익스플로잇이 동시 진행되는 경우)에서 두 가지 한계가 제기되었다:

1. **중첩 병렬성(Nested Parallelism)의 부재:**
   - Depth 1 워커가 거대한 하위 과업(예: "웹 서비스 전체 정찰")을 배정받았을 때, 하위 워커를 두어 엔드포인트별/가설별로 추가 병렬화하지 못하고 순차 처리해야 하는 병목이 발생함.
2. **정보 왜곡(전화 게임, Chinese Whispers) 위험:**
   - 다계층 트리로 확장할 때 중간 노드가 하위 리프 노드의 기술적 세부사항을 모호하게 요약("오프셋을 찾았습니다" 등)하여 상위 루트의 올바른 익스플로잇 판단을 방해할 위험이 존재함.

---

## 3. 결정

### 3.1 3-Depth 한정 계층 토폴로지 (Bounded 3-Depth Hierarchy)

무한 재귀 트리(Unbounded Tree)의 스폰 폭주와 통제 불능 위험을 원천 차단하기 위해 **깊이는 정확히 최대 3단계(Depth 0, 1, 2)**로 제한한다.

```text
[ Root / Main ] (Depth 0: 팀장)
  ├── [ Lead-01 / Worker-01 ] (Depth 1: 중간 리드 또는 단독 워커)
  │     ├── [ Worker-01-A ] (Depth 2: 리프 워커, 더 이상 하위 생성 불가)
  │     └── [ Worker-01-B ] (Depth 2: 리프 워커, 더 이상 하위 생성 불가)
  └── [ Worker-02 ] (Depth 1: 단독 리프 워커)
```

- **Depth 0 (Root / Main):** 유일한 총괄 팀장. 전체 전장 지휘, Depth 1 에이전트 생성/배정/회수, 최종 목표 보고(`report final`).
- **Depth 1 (Intermediate Lead / Worker):** Main이 생성. 하위 Depth 2 워커를 생성(`team create`)하여 위임하거나, 자식 없이 단독 실행할 수 있음.
- **Depth 2 (Leaf Worker):** Depth 1 리드가 생성. **더 깊은 하위 에이전트 생성은 런타임에서 거부**됨. 실제 도구 실행을 전담.
- **활성 팀 크기 상한:** 전체 활성 에이전트 수는 계층에 관계없이 **Main 포함 최대 10명**으로 유지(ADR-0001 §5 정합).

---

### 3.2 역할에 따른 프롬프트 및 도구 호출 동적 분기

에이전트의 시스템 프롬프트는 자신의 **Depth, 자식 노드 유무, 부모 노드 유무**에 따라 동적으로 구성된다:

1. **내부 노드 (자식 노드가 있는 에이전트):**
   - **역할:** 과업 분할(Scatter), 자식 지휘 및 결과 취합(Gather), 전장 요약(`brief.md`), 상위 보고.
   - **도구 행동 지침:** 자식이 있는 동안은 **직접 도구(`shell`, `workspace`) 실행을 지양**하고, 하위 자식에게 실행을 위임하여 자신의 문맥이 대용량 터미널 출력으로 오염되는 것을 방지함.
2. **리프 노드 (자식 노드가 없는 에이전트 / Depth 2 전원):**
   - **역할:** 실제 환경과 상호작용하는 **유일한 도구 실행 주체**.
   - **도구 행동 지침:** 즉시 도구를 실행(`shell`, `workspace`)하고 가설을 검증하며, 하위 생성을 시도하지 않고 기술적 결과를 직속 부모에게 직통 보고.

---

### 3.3 정보 왜곡 방지 및 원시 값 보존 (Anti-Chinese-Whispers Protocol)

다단계 요약 과정에서 공격에 필수적인 정밀 데이터가 손실되지 않도록 엄격한 프롬프트 규약을 적용한다:

- **리프 노드의 보고 의무:** 추상적인 산문("취약점 발견함")을 금지하고, **정확한 식별 값(Exact Distinguishing Values: 정확한 엔드포인트, 포트, 메모리 오프셋 `0x...`, Leak 바이트, 추출된 자격증명 `user:pass`, 토큰, 플래그 문자열)**을 반드시 포함하여 보고한다.
- **중간 노드의 취합 의무:** 하위 리프들이 보고한 원시 값과 구체적 기술 팩트를 **임의로 삭제하거나 추상화하여 요약하지 않고 온전히 보존(Exact-Value Retaining Synthesis)**하여 상위 노드로 전파한다.

---

### 3.4 직계 통신 한정 불변식 (Strict Immediate-Scope Communication)

메시지 버스의 복잡도와 무분별한 스팸을 방지하기 위해 통신 가능 범위를 엄격히 제한한다:

```text
[ 허용되는 통신 경로 ]
1. 직계 부모 ↔ 직계 자식 (Immediate Parent-Child)
2. 동일 부모를 둔 직계 형제 ↔ 직계 형제 (Immediate Siblings)

[ 금지되는 통신 경로 (런타임 거부) ]
1. 조부모 ↔ 손자 (Skip-level communication: Main ↔ Depth 2 직접 메시지 금지)
2. 사촌 간 (Cross-branch communication: 다른 부모를 둔 워커 간 직접 메시지 금지)
```

- 하위 Depth 2의 중요한 통찰(`Insight`) 및 최종 요약(`Final`)은 **직속 부모(Depth 1)에게 전달되고, 부모가 이를 합성하여 Main(Depth 0)으로 보고**하는 계층적 집계 방식을 따른다.

---

## 4. 불변식 (Invariants)

1. **최대 깊이 불변식:** `depth <= 2`. Depth 2 에이전트의 `team create` 호출은 `DomainError::InvalidDepth`로 즉각 거부한다.
2. **팀 상한 불변식:** 전체 non-terminal 에이전트 합계 $\le 10$.
3. **직계 통신 불변식:** 발신자와 수신자는 (1) 부모-자식 관계이거나 (2) 부모가 동일한 형제 관계여야만 저널 append 및 Inbox 투영이 성립한다.
4. **단일 원장 불변식:** 모든 계층의 메시지와 이벤트는 여전히 **단 하나의 Run Journal**에 순차 기록된다.

---

## 5. 구현 로드맵 (0.112.0)

1. **`crates/ma-core/domain.rs`**: `AgentDepth`를 `Main(0)`, `Lead(1)`, `Leaf(2)`로 확장 또는 `u8` 기반 검증(`depth <= 2`) 적용.
2. **`crates/ma-coordinator/lib.rs`**: 에이전트 레코드에 `parent_id` 필드 추가, 직계 부모-자식 및 형제 통신 검증 로직 반영.
3. **`crates/ma-runtime/src/runtime.rs`**: `build_system`에서 깊이 및 자식 유무에 따른 프롬프트 분기 렌더링.
4. **`tests/coordinator_contract.rs` & `tests/runtime_contract.rs`**: 3-depth 생성, 권한 위임, 통신 범위 제한 계약 검증 테스트 추가.

---

## 6. 사용자 확정 보완 (2026-09-01)

본 ADR의 §3 결정에 더해, 논의에서 다음을 확정한다.

- **자원 상한 = 전역 총 10명 유지(폭발 상한 불필요).** 사용자 판정: 3단×노드10=최대 111도
  과하지 않으나, 현재 전역 permit 상한(총 활성 ≤10)을 그대로 둔다 — 별도 전역 폭발 상한을
  새로 심지 않는다. (§3.1/§4-2와 정합.)
- **죽음 감지 + 고아 방지(캐스케이드).** 의도적 recall/terminal은 서브트리 전체를 함께
  종료한다. 더해, 중간·자식 노드의 **비정상 종료를 감지**해 고아가 된 서브트리를 회수·
  재배정한다 — 주인 없는 손자를 남기지 않는다(ADR-0001 kill-on-drop 정합).
- **재시작 복구 = A-light (확정, 2026-09-01).** 방식 A(루트만 재위임)를 그대로 구현하면
  ADR-0001의 "재시작 시 워커가 durable inbox·session·context와 함께 resume" 불변식과 정면
  충돌한다(runtime_contract 다수 테스트로 강제; 방식 A는 그 핵심 능력을 삭제해야 함). 그래서
  사용자 확정에 따라 **A-light**로 간다: 재시작은 **ADR-0001대로 워커를 resume**하되(durable
  resume 보존), **깊은 손자 구조는 재구성하지 않는다**(recover가 비-main을 main의 직속 자식으로
  flatten). "무거운 복구 머신 금지"라는 방식 A의 취지를 만족하면서 멀쩡한 durable resume는
  버리지 않는다. 코어는 별도 변경 없이 이 동작을 만족한다.
- **에이전트 자산 언어 = 영어.** `prompts/*.md`, `prompts/skills/*.md`는 영어로 저술.
- **코드 매핑:** `AgentDepth`는 `u8` 경계 newtype(0..=2), depth2 생성은
  `DomainError::MaxDepthReached`로 거부(§5 로드맵의 `InvalidDepth`는 이 이름으로 구현).

## 7. 증분 상태 (2026-09-01, Docker 게이트 green)

- **increment A — 완료·검증:** `AgentDepth` 3단 `u8` newtype(0..=2, `MAIN`/`is_main`/
  `can_spawn`/`child`) + `AgentRecord`/`AgentSnapshot`의 `parent` 추적 + 생성 시 depth=부모+1
  (depth<2만, 리프 생성은 `MaxDepthReached`) + `send`의 직계-이웃(부모∨자식∨형제) 검증
  (`NonNeighborAudience`, main 자동포함·`MissingMainAudience` 폐기) + `require_main` 정정
  (`CallerNotMain`). 런타임: `team finish`가 빈 audience 대신 **부모로 버블업**. 계약 테스트 갱신.
- **increment B — 완료·검증:**
  - **죽음 감지 + 캐스케이드:** `mark_terminal`이 서브트리 전체를 Stopped로 캐스케이드(fault로
    비정상 종료한 노드의 서브트리도 회수 = 죽음 감지), `recall`이 서브트리를 Recalling+취소로
    캐스케이드. `collect_descendants`(parent 포인터 BFS) + `cascade_subtree` 헬퍼.
    - **감사(코드리뷰)로 확인·보완:** 캐스케이드 계약을 테스트로 고정
      (`terminating_or_recalling_an_internal_node_cascades_to_its_whole_subtree`,
      `recalling_an_internal_node_cascades_recall_to_its_subtree` — 첫 구현 시 누락됐던 것).
      **죽음 감지 범위 한계(정직히 명시):** worker_driver가 회복불가 fault를 `Faulted`로,
      recall을 `Stopped`로 mark_terminal → 그 경로들은 서브트리를 회수한다. 그러나 worker task가
      worker의 unwind panic도 수명 종료 guard가 기존 mark_terminal 경로로 넘겨 하위 트리와 슬롯을 회수한다. 프로세스 abort·강제 종료는 destructor가 실행되지 않으므로 이 보장 밖이며 durable resume 대상이다.
  - **위치 기반 프롬프트 배선:** `build_system`이 POSITION 블록(depth·리프/내부·부모·자식·형제)을
    주입하고 `team-tree.md` 상시 + 비-main에 `node-internal.md`(자식 있음)/`node-leaf.md`(리프)를
    선택 삽입.
  - **recover = A-light**(위 §6). 코어 추가 변경 없음.
- **테스트:** 전체 워크스페이스 `cargo test` green(172 passed / 0 failed, 단일 스레드).
- 남은 승격 조건: 리버스셸 등 실 교전 E2E 스모크(ADR-0003 §6와 공유)가 green이면 Accepted 승격
  - 0.111.0 릴리스. 그 전까지 Proposed, 버전 0.110.0 유지.
