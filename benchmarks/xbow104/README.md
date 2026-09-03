# XBOW-104 벤치마크 하니스

XBOW-104는 [xbow-engineering/validation-benchmarks](https://github.com/xbow-engineering/validation-benchmarks)
(Apache-2.0)에서 공개한 104개 단일 플래그 웹 보안 CTF 과제에 대한 자율 에이전트 벤치마크 하니스입니다.

## 빠른 시작

```bash
# 1. 벤치마크 스위트 클론
git clone https://github.com/xbow-engineering/validation-benchmarks \
  benchmarks/xbow-validation-benchmarks

# 2. 프로젝트 루트에 .env 구성 (.env.example 참고)
cp benchmarks/xbow104/.env.example .env
# → 에디터로 열어서 백본 토큰 채우기

# 3. 환경 진단
npm run xbow:doctor

# 4. 에이전트 러너 이미지 빌드
npm run xbow:setup

# 5. 단일 태스크 실행 (42번)
npm run xbow -- 42

# 6. 전체 104개 태스크 실행
npm run xbow:run
```

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
| `--concurrency <N>` | `1` | 병렬 실행 수 (공식 런은 반드시 1) |
| `--timeout <초>` | `3600` | 태스크당 제한 시간 |
| `--no-hints` | 힌트 포함 | description 힌트 제외 |
| `--rerun-all` | 건너뜀 | 이미 완료된 태스크도 재실행 |
| `--no-commit` | 자동커밋 | git 자동 커밋/푸시 비활성화 |

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

## 관련 문서

- [벤치마크 방법론](../../docs/BENCHMARK_METHODOLOGY.md) — 5단계 파이프라인
- [증거 보존 계약](EVIDENCE-RETENTION.md) — 불변성 및 감사 규칙
