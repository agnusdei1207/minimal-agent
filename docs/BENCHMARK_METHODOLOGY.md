# 벤치마크 방법론 — 데이터 기반 개발 연구

작성일: 2026-09-01 · 상태: 규범(v1)

> **벤치마크는 단순 수치 체크가 아니다.** 실행은 시작일 뿐이고, 목적은 결과를
> 분석해 **시스템의 개선 통찰**을 꺼내는 것이다.

## 0. 원칙

- 벤치마크 = **데이터 기반 개발 연구(data-driven development research)**.
  점수는 신호일 뿐, 산출물은 **통찰과 개선 백로그**다.
- 모든 실험은 하나의 사이클을 완주한다:
  **실행 → 분석 → 분류 → 통합 → 도입 필터링 → 다음 설계 반영.**

## 1. 5단계 파이프라인

| 단계 | 이름 | 하는 일 | 산출물 |
|---|---|---|---|
| 1 | **실행(Run)** | 고정 백본·재현 가능 하니스로 전체 스위트 실행 | `runs/<task>/evidence.json`, transcript, telemetry |
| 2 | **개별 분석(Analyze)** | 태스크별 트랜스크립트 완전 분석 — 정찰→시도→결과 | `reports/per-run/XBEN-*.md` |
| 3 | **근본원인 분류(Classify)** | 각 결과를 §2 taxonomy로 판정 | 각 리포트의 "근본원인" 섹션 |
| 4 | **통합/집계(Aggregate)** | 개별 분류를 가로질러 반복 패턴으로 통합 | `reports/FINDINGS.md` |
| 5 | **도입 필터링(Filter→Adopt)** | 실제 도입할 개선안을 우선순위로 필터링 | `reports/ADOPTION_BACKLOG.md` |

## 2. 근본 원인 분류 taxonomy

각 결과를 아래 넷 중 하나로 판정하고 근거를 남긴다.
핵심 질문: **"모델을 바꿔야 하나, 우리 설계만 바꿔도 되나?"**

- **MODEL** — LLM 자체의 추론·지식 한계. 더 강한 모델이면 풀렸을 것.
- **SYSTEM-DESIGN** — 우리 런타임 설계 문제. 프롬프트, 툴 구성, 반복 억제, 컨텍스트 관리 등.
  **모델을 그대로 두고 설계만 바꿔도 개선될 것들 — 가장 값진 통찰.**
- **DIFFICULTY** — 순수 난이도. 설계·모델과 무관하게 시간 내 어려움.
- **INFRA** — 벤치 타깃/하니스 결함. **점수에서 제외**하고 별도 표기.

## 3. 폴더 구조 템플릿

```text
benchmarks/xbow104/artifacts/
├─ runs/                         # 1. 실행 raw — 불변 증거 레이어
│   └─ <TASK>-<stamp>/           #    evidence.json · transcript.txt · telemetry/ · harness/
└─ reports/
    ├─ SUMMARY.md · KPI.md · results-index.json   # 집계 지표
    ├─ per-run/                  # 2·3. 개별 분석 + 근본원인 분류
    │   ├─ _TEMPLATE.md          #    구조적 템플릿
    │   ├─ <TASK>.md             #    태스크별 분석 (한국어)
    │   └─ README.md             #    롤업 인덱스 + 분류 집계표
    ├─ FINDINGS.md               # 4. 통합/집계
    └─ ADOPTION_BACKLOG.md       # 5. 도입 필터링
```

## 4. 산출물 정의

- **per-run/_TEMPLATE.md** — 고정 섹션: ①한눈요약 ②타깃개요 ③전체 트랜스크립트 분석 ④잘한 점 ⑤문제점(정량) ⑥근본원인(§2) ⑦시스템 개선 액션.
- **SUMMARY.md** — 난이도별 성공률, 토큰, 비용 종합 표.
- **KPI.md** — 턴 수, 턴당 소요시간, p50/p95, 툴 실패율 등.
- **FINDINGS.md** — 개별 리포트의 개선 제안을 주제별로 통합.
- **ADOPTION_BACKLOG.md** — FINDINGS 중 도입할 것만 필터링 (우선순위·상태).

## 5. 피드백 루프

`ADOPTION_BACKLOG.md`의 채택 항목 → 시스템 변경 → 다음 실험에서 **동일 지표로 재측정** → 개선 여부를 누적.
실험이 개발을 이끈다.
