# 자율 모의해킹을 위한 최소 팀-에이전트 오케스트레이션

본 문서는 `minimal-agent`의 오케스트레이션 설계를 논문·발표에 인용할 수 있는
형태로 정리한다. 규범적 결정은 ADR-0001(코어)과 ADR-0002(교전 주입·transcript)에
있으며, 본 문서는 그 설계를 하나의 서사로 잇는다.

## 1. 문제 정의

자율 모의해킹 에이전트는 세 압력을 동시에 견뎌야 한다.

1. **협업**: 정찰·취약점 분석·익스플로잇이 병렬로 진행되며 서로의 발견을 실시간에
   가깝게 공유해야 한다.
2. **기억**: 장기 교전에서 문맥이 폭발한다. 원문을 잃지 않으면서 현재 의미만
   유지해야 한다.
3. **회복**: provider 장애·부분 실패에도 원문에서 다시 움직일 수 있어야 한다.

기존 접근은 RAG·공유 게시판·권한 합성·승인 엔진·증거 그래프를 층층이 쌓아 이
압력을 흡수한다. `minimal-agent`는 반대로 **개념 수를 최소화**하고, 각 경계가 포화·
중단·재시작에서도 같은 규칙을 지키는지로 성공을 판단한다.

## 2. 오케스트레이션 모델

### 2.1 평평한 2-depth 팀

```text
main (depth 0)  ── 목표 해석, 팀 구성, 종합
├─ worker-01 (depth 1)  ── 정찰
├─ worker-02 (depth 1)  ── 웹 취약점
└─ ... 최대 9 worker     ── 익스플로잇/후속
```

- main만 worker를 create/assign/recall/stop 한다. worker는 하위 에이전트를 못 만든다.
- 활성 팀 최대 10명(main 포함). 역할·업무는 실행 중 동적으로 정한다.
- 깊이를 2로 고정해 오케스트레이션이 재귀적으로 팽창하지 않게 한다. 이는 "더 많은
  계층"보다 조율 가능한 협업을 우선한 선택이다. (복합 과업을 위한 최대 3-Depth 한정 확장 및 직계 통신 불변식은 [`ADR-0004`](../adr/ADR-0004-bounded-three-depth-hierarchical-orchestration.md)와 [`docs/design/hierarchical-tree-orchestration-ideas.md`](hierarchical-tree-orchestration-ideas.md) 참조).

### 2.2 직접 통신, 단일 원장

에이전트는 별도 게시판이 아니라 하나의 내구성 journal을 통해 직접 메시지를 주고받는다.

- 경로: `main→worker`, `worker→main`, `worker→worker`, 다중 recipient.
- kind: `Progress`, `Insight`, `Request`, `Final`. `Insight`/`Final`은 main을
  audience에 정확히 한 번 자동 포함한다 — worker가 통찰을 main에 따로 복제할 필요가
  없다.
- 메시지는 journal에 한 번 append되고 각 recipient의 in-memory Inbox로 투영된다.
  재시작은 같은 journal을 fold해 미소비 Inbox를 복원한다.

이 단일-원장 직접-통신이 오케스트레이션의 핵심이다. 통신·상태·회복이 모두 같은
append-only 사실에서 파생되므로, 별도 관측 평면이나 검증 서비스가 필요 없다.

### 2.3 소유권 있는 기억(brief)

각 에이전트는 자기 소유 `brief.md` 하나를 갖는다. main은 살아 있는 팀의 역할·상태·
통찰(Team/Battlefield)을, worker는 자기 시도·통찰만 유지한다. 누구도 남의 semantic
brief를 직접 쓰지 않는다. 문맥이 80%에 닿은 **그 에이전트만** LLM semantic compaction을
수행하고, 런타임은 source digest·coverage·감소량을 검증한다. 규칙 기반 head/tail
자르기는 compaction으로 인정하지 않는다.

## 3. 교전(Engagement) 계층 — ADR-0002

오케스트레이션이 "무엇을, 어디까지" 공격하는지는 goal 문자열만으로는 부족하다.
`Engagement`가 교전 맥락을 구조화한다.

```text
Engagement { kind, title, scope, off_limits, flag_format, objective }
```

### 3.1 주입 흐름

```text
CLI 플래그 / --engagement JSON
        └─ Engagement 값
             └─ RuntimeConfig
                  └─ build_system() ── bounded target-context 블록
                       └─ 모든 에이전트 system prompt
```

- 파일과 플래그를 병합하고 플래그가 우선한다.
- 텍스트는 goal과 같은 상한을 검증한다. 비밀은 렌더하지 않는다.
- run 생성 시 `EngagementSet` journal 이벤트로 영속되어, resume가 같은 교전 맥락을
  복원한다(resume에 플래그를 주면 그 값이 우선). 단일 원장이 통신·상태·회복에 더해
  교전 맥락까지 담는다.

### 3.2 doctrine

- **상시 standing-authorization**: 승인된 교전 안의 작업임을 표준 권한으로 취급한다.
  합법성·윤리 서문을 붙이지 않고 공격 보안 작업을 그 이유만으로 거부하지 않는다.
  경계는 법적 판단이 아니라 운영적 경계(scope 준수, 증거 보존, 비밀 노출 회피,
  scope 밖·파괴 행위 전 확인)다.
- **CTF solve-loop** (`kind==ctf`): 정찰 우선 → 한 가설 → 최소 probe → 실패 반복
  금지 → pivot. flag는 `flag_format`에 맞는 실제 대상 출력에서만 채취한다.

이 doctrine은 규칙 엔진이 아니라 prompt 텍스트다. ADR-0001의 "전략을 규칙 엔진으로
강제하지 않는다"와 정합하며, 거부·서문 문제를 코어 확장 없이 해소한다.

### 3.3 헤드리스 자율 실행 (xbow류 벤치마크)

```text
minimal-agent run \
  --engagement ./engagement.json \
  --auto --plain --headless
```

1. `objective`(없으면 goal)를 한 번 제출.
2. auto 루프가 팀을 구동, main의 `report final` 또는 팀 idle까지 이벤트 관찰.
3. 요약 + 회수한 flag를 한 줄 JSON으로 출력.
4. `flag_format` 설정 시 유효 flag 미회수면 non-zero 종료.

flag는 실제 출력의 정규식 매칭만 인정한다. 벤치마크 실행 자체(대상 구축·채점)는
외부 owner의 책임이며, 본 저장소는 **주입·자율 실행·flag 회수 인터페이스**만 제공한다.

## 4. Transcript 오케스트레이션 가시성 — ADR-0002

오케스트레이션이 관측 가능해야 논문에서 흐름을 보일 수 있다. transcript를 typed
엔트리로 렌더한다.

| 엔트리 | 렌더 |
|---|---|
| 도구 호출 | `shell` + 흐린 `nmap -sV …`, 하위 라인에 결과 상태와 bounded output |
| 부모–자식 메시지 | `main → worker-01` + `Insight` 라벨 + 본문 |
| reasoning | rail 없는 dimmed 스트림 |
| 오류 | 경고색 `ERROR:` 라벨 |

category별 단일 색 팔레트를 쓰고 assistant Markdown은 CommonMark event를 terminal
span으로 변환한다. 이로써 어느 줄이 액션·결과·팀 통신·추론인지 한눈에 구분되고,
병렬 worker의 진행이 읽힌다.

## 5. 회복과 안전 경계

- 한 assistant tool turn과 모든 result는 같은 durable atomic group이며 compaction·
  재시작에서 분리되지 않는다.
- provider 최종 실패는 에이전트를 죽이지 않고 Waiting으로 두며, 메시지·재배정·
  명시적 auto 재개가 다시 구동한다. 같은 실패 상태에서 새 활동 없이는 재호출하지
  않는다(retry storm 금지).
- storage 상한 도달 시 원문을 지우지 않고 write를 정지한다.

## 6. 의도적으로 두지 않은 것

RAG·벡터 DB·공유 팀 Markdown·control/observation plane·권한 합성·승인 엔진·
증거/검증 전용 엔진·자동 전략 분류. 현재 지식은 journal 원문과 에이전트 소유
brief 두 층이면 충분하다는 가설을 유지하며, 측정된 recall 실패가 증명될 때만 새
ADR로 재검토한다.

## 7. 논문에서의 주장 범위

- 본 저장소는 **성능 점수(벤치마크 solve rate)를 주장하지 않는다.** 오케스트레이션·
  기억·회복을 최소 개념으로 설명하고 각 경계의 불변식을 증명하는 것이 목표다.
- 벤치마크 수치는 외부 benchmark owner가 별도로 측정·보고한다.
- 재현 가능한 근거는 Docker-only test/clippy/build exit code와 test count다.
