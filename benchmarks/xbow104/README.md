# XBOW-104 벤치마크 하니스

XBOW-104는 [xbow-engineering/validation-benchmarks](https://github.com/xbow-engineering/validation-benchmarks)
(Apache-2.0)에서 공개한 104개 단일 플래그 웹 보안 CTF 과제에 대한 자율 에이전트 벤치마크 하니스입니다.

> **운영 시 반드시 [RUNBOOK.md](RUNBOOK.md)를 따르세요** — 표준 실행 절차, 5분
> 간격 서브에이전트 모니터링, 용량 관리, 에스컬레이션이 정리돼 있어 매번 지시할
> 필요가 없습니다. 빌드 시로트 원인·수정은 [BUILD-INFRA.md](BUILD-INFRA.md).
> 다른 에이전트에는 [HANDOFF.md](HANDOFF.md)의 프롬프트를 전달하세요.

## 빠른 시작

```powershell
# 1. 벤치마크 스위트 클론
git clone https://github.com/xbow-engineering/validation-benchmarks benchmarks/xbow-validation-benchmarks

# 2. 프로젝트 루트에 .env 구성 (.env.example 참고)
if (-not (Test-Path .env)) { Copy-Item benchmarks/xbow104/.env.example .env }
# → 에디터로 열어서 백본 토큰 채우기

# 2.5 스위트 빌드 인프라 시로트 수정 (필수, idempotent)
#     EOL Debian 미러/삭제된 phantomjs/잘못된 expose 문법 등 시간 경과로
#     빌드 불가가 된 챌린지를 되살린다. 자세한 원인·수정은 BUILD-INFRA.md 참고.
& ./scripts/nverify.ps1 -HostNode -NodeArguments @('benchmarks/xbow104/patch-suite.mjs')

# 3. 환경 진단
& ./scripts/nverify.ps1 -HostNode -NodeArguments @('benchmarks/xbow104/doctor.mjs')

# 4. 에이전트 러너 이미지 빌드
& ./scripts/dimage.ps1 -Target runner -Tag xbow-agent-runner:latest

# 5. 단일 태스크 실행 (42번)
& ./scripts/nverify.ps1 -HostNode -NodeArguments @('benchmarks/xbow104/task.mjs', '42', '--no-commit')

# 6. 전체 104개 태스크 실행
& ./scripts/nverify.ps1 -HostNode -NodeArguments @('benchmarks/xbow104/runner.mjs', '--all', '--concurrency', '1', '--timeout', '900', '--no-commit')
```

이미 `.env`가 있으면 덮어쓰지 않습니다. 공유 suite를 사용하는 라이브 실행/빌드가 있다면
패치·이미지 재빌드·중복 실행은 하지 않습니다. 아래 npm 별칭은 대응 관계 안내이며,
실제 실행은 위 Node 24 검증 래퍼 또는 dimage를 사용합니다.

## 명령어 일람

| 명령어 | 설명 |
|---|---|
| `npm run xbow -- <N>` | 단일 태스크 실행 후 리포트 갱신 |
| `npm run xbow -- <N> --dry-run` | 실행 계획만 출력 |
| `npm run xbow:run` | 미완료 태스크 전체 순차 실행 |
| `npm run xbow:run -- --rerun-all` | 전체 104개 강제 재실행 |
| `npm run xbow:setup` | 에이전트 러너 이미지 빌드 |
| `npm run xbow:doctor` | 환경 사전 진단 |
| `npm run xbow:summary` | SUMMARY.md 재생성 |
| `npm run xbow:kpi` | KPI.md + kpi.json 재생성 |

## 실행 옵션

| 옵션 | 기본값 | 설명 |
|---|---|---|
| `--tasks <ID>` | — | 특정 태스크 (콤마 구분 복수 가능) |
| `--level <1\|2\|3>` | — | 특정 난이도만 필터링 |
| `--all` | — | 104개 전체 실행 |
| `--concurrency <N>` | `1` | 1~5 정수. 여러 러너의 활성 태스크 합계도 5 이하 |
| `--timeout <초>` | `3600` | 태스크당 제한 시간 |
| `--no-hints` | 힌트 포함 | description 힌트 제외 |
| `--rerun-all` | 건너뜀 | 이미 완료된 태스크도 재실행 |
| `--no-commit` | 자동커밋 | 명시적인 커밋/푸시 승인이 없는 세션에서는 필수 |

## 산출물 구조

```text
artifacts/
├─ runs/                         # 불변 증거 레이어
│   └─ XBEN-042-24-<timestamp>/
│       ├─ prompt.txt            # 에이전트에 전달된 프롬프트
│       ├─ transcript.txt        # 전체 표준입출력 (무손실)
│       ├─ evidence.json         # 결과, 토큰, 소요시간
│       ├─ run-state.json        # 진행 상태
│       ├─ audit-manifest.json   # SHA-256 무결성 검증
│       ├─ harness/              # Docker 빌드/기동/종료 로그
│       └─ telemetry/            # 런타임 JSONL 이벤트 로그
└─ reports/                      # 투영 레이어 (재생성 가능)
    ├─ SUMMARY.md                # 종합 요약 (논문 스타일)
    ├─ KPI.md                    # 정량/정성 진단 지표
    ├─ kpi.json                  # KPI JSON 데이터
    ├─ results-index.json        # 최신 유효 실행 인덱스
    ├─ attempt-history.json      # 전체 시도 이력
    └─ per-run/                  # 태스크별 심층 분석 리포트
```

## 플래그 생성 규칙

```
FLAG{sha256(TASK_ID.toUpperCase())}
```

하드코딩 없이 태스크 ID에서 동적으로 생성됩니다.

## 타깃과 집계 주의사항

- XBEN-020은 공개 앱 `ssrf-demo-app`으로 접근합니다. `internal-service`의 expose는 의도된
  SSRF 격리이며 포트 공개로 고치지 않습니다. 세 러너는 공통 target selector를 사용합니다.
- 재시도 원본은 모두 보존합니다. XBOW 공통 집계는 최신 유효 attempt, provider별 summarize는
  최신 완료 attempt를 선택합니다. provider 소비량은 보존된 전체 완료 attempt를 합산합니다.
- 미측정 지표는 null/-입니다. 정상 복구된 429, timeout, 빈 네트워크만으로 infra를 단정하지
  않습니다. 완료된 런의 보고서 재생성은 실행 소유자가 맡습니다.

## 관련 문서

- [벤치마크 방법론](../../docs/benchmark/METHODOLOGY.md) — 5단계 파이프라인
- [증거 보존 계약](EVIDENCE-RETENTION.md) — 불변성 및 감사 규칙

관련 변경 설명: [벤치마크 신뢰성 개선 보고서 — 원인·변경 전후·검증](../../docs/benchmark/reliability-2026-09-05.md)
