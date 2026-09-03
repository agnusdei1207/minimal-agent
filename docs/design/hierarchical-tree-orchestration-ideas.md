# 계층적 트리 오케스트레이션 설계 탐색 노트 (Hierarchical Tree Exploration)

본 문서는 `minimal-agent`의 현행 **스타형 단일 레벨 팀(1 Main + ≤9 Workers, Depth 1)** 구조를 넘어, **임의 깊이의 재귀적 트리(Unbounded Recursive Tree) 및 내부 노드(위임·요약 전용) / 리프 노드(도구 실행 전용) 분리 모델**에 대한 아키텍처적 가능성과 트레이드오프를 탐색·기록한다.

---

## 1. 제안된 아키텍처 모델: "감독자-작업자 순수 계층 트리"

### 1.1 핵심 규칙 정의
1. **역할 분리 (Strict Internal vs. Leaf Separation):**
   - **내부 노드 (Internal / Supervisor Node):** 자식 노드가 존재하는 에이전트. **직접 도구(`shell`, `workspace`)를 호출하지 않으며**, 오직 하위 자식 노드로 과업을 분할(Scatter/Delegate)하고, 자식들의 보고를 취합·요약(Synthesize/Reduce)하여 상위 부모 노드에 보고한다.
   - **리프 노드 (Leaf / Worker Node):** 자식 노드가 없는 최하위 실행 에이전트. **실제 환경과 상호작용하는 유일한 주체**로서 도구를 실행하고, 그 기술적 결과를 직속 부모 노드에 보고한다. 하위 노드를 생성할 수 없다.
2. **단방향 수직 통신 및 요약 전파:**
   - 명령은 상위에서 하위로 흐르고, 결과와 통찰은 하위에서 상위로 요약되어 올라간다.

```text
                  [ Root / Main ] (Depth 0: 총괄 전략 & 최종 보고)
                   /             \
       [ Track Lead A ]        [ Track Lead B ] (Depth 1: 과업 분할 & 요약)
        /            \              |
  [ Leaf W1 ]    [ Leaf W2 ]    [ Leaf W3 ] (Depth 2+: 실제 도구 실행)
  (tool: shell)  (tool: shell)  (tool: shell)
```

---

## 2. 알고리즘적 분류 및 특성

| 관점 | 스타 구조 (현행 minimal-agent) | 순수 계층 트리 (탐색 모델) |
| :--- | :--- | :--- |
| **트리 형태** | $K_{1, n}$ (높이 1, 방사형 스타 그래프) | $N$-ary Tree (임의 높이 계층 트리) |
| **알고리즘 패턴** | Single-Level MapReduce / Scatter-Gather | Multi-Stage Divide-and-Conquer / Actor Supervision Tree |
| **도구 실행 주체** | Main(직접 실행 가능) + Worker(실행) | **오직 Leaf 노드만 실행** |
| **중간 노드 역할** | 없음 (Main 1명이 직접 전원 지휘) | 분할(Partition) + 요약(Reduce) + 중계(Relay) |
| **상한(Bounds)** | Active Team $\le 10$, Depth $= 1$ 고정 | Depth $\infty$ 또는 Bounded Depth $D$, Subtree Cap |

---

## 3. 기대되는 장점 (Pros)

1. **역할의 극단적 명확성 및 컨텍스트 순도 유지:**
   - 관리 노드가 도구 실행 결과(대용량 출력)로 인해 자신의 문맥을 오염시키지 않고, 오직 상위 전략과 요약된 통찰(`brief.md`)만 다루므로 환각과 방향 상실이 감소함.
2. **초대형 복합 과업으로의 확장성:**
   - 단일 에이전트가 10명 이상의 워커를 인지적으로 관리하기 어려운 한계(Cognitive Span of Control)를 극복하여, 서브 프로젝트별 리드에게 관리를 위임할 수 있음.
3. **자연스러운 계층적 압축 (Hierarchical Compaction):**
   - 최하위의 수백 줄 로그 $\to$ 중간 리드의 3줄 통찰 $\to$ 루트의 1줄 전장 상황으로 상향식 의미 압축이 자연스럽게 발생함.

---

## 4. 치명적인 위험과 트레이드오프 (Cons & Challenges)

1. **지연 시간(Latency) 및 턴 수의 지수적 폭발:**
   - 리프 노드의 발견이 루트에 도달하려면 최소 $D$번의 순차적 LLM 턴(Leaf $\to$ Lead $\to$ Root)이 직렬로 발생함.
   - 단순한 1줄 확인 작업(예: "포트 80 서비스 버전 확인")조차 생성 $\to$ 위임 $\to$ 실행 $\to$ 보고 $\to$ 요약으로 최소 4~6턴이 소모됨.
2. **정보의 전화 게임 (Information Degradation / Loss of Precision):**
   - 공격 보안(CTF/익스플로잇)에서는 **정확한 1바이트 릭(Leak) 값, 메모리 오프셋, Nonce 값**이 치명적인 핵심인데, 다단계 요약을 거치면서 "오프셋이 발견됨" 수준의 추상화된 문장으로 뭉개져 루트에서 올바른 판단을 내리지 못할 위험.
3. **위임 핑퐁 및 스폰 폭주 (Spawn Storms & Infinite Delegation):**
   - 모델이 어려운 문제에 직면했을 때 스스로 해결하지 않고 계속해서 하위 자식 노드를 스폰하는 회피성 위임 루프에 빠져 비용/토큰이 폭발할 위험.
4. **장애 복구 및 원장(Journal) 관리 복잡도:**
   - 중간 노드가 비정상 종료되거나 타임아웃되었을 때, 그 하위 서브트리 전체의 가비지 컬렉션, 세마포어 회수, 저널 리플레이 복원의 복잡도가 기하급수적으로 증가함.

---

## 5. 실전 CTF 및 모의해킹 관점에서의 평가 및 최종 결정

- **CTF의 실제 특성:** 대부분의 CTF 및 목표 지향적 침투는 **수평적 병렬성(Horizontal Breadth: 포트 5개 동시 정찰, 웹/바이너리/크립토 가설 동시 검증)**이 핵심이나, 복합 과업(서브넷 정찰 후 특정 취약점 집중 공략 등)에서는 1단계의 중첩 병렬화가 실질적인 가치를 제공함.
- **최종 아키텍처 결정 (ADR-0004 채택):**
  - 무한 재귀의 위험을 원천 차단하기 위해 **최대 3-Depth (Root $\to$ Intermediate Lead $\to$ Leaf Worker)**로 상한을 명확히 고정.
  - 정보 왜곡(전화 게임)을 막기 위해 **핵심 원시 값(Exact Distinguishing Values) 보존 프로토콜**을 프롬프트에 강제.
  - 통신 범위를 **직계 부모-자식 및 직계 형제 간으로 엄격히 한정**하여 메시지 버스의 복잡도 통제.
  - 상세 아키텍처 사양은 [`docs/adr/ADR-0004-bounded-three-depth-hierarchical-orchestration.md`](../adr/ADR-0004-bounded-three-depth-hierarchical-orchestration.md) 참조.
