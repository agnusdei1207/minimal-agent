# XBOW-104 결과 보고 — 2026-09-03 실행분

> 수작성 포스트모템. `SUMMARY.md` / `KPI.md` / `results-index.json` / `attempt-history.json`은
> 집계기 산출물(재생성 명령은 §5)이며, 이 문서는 그 해석이다.

## 1. 요약

- **스코어링 가능한 시도: 0건** → 공식 solve rate 산출 불가. 14회 시도 전부
  인프라/런타임 결함으로 종료되어 `valid_for_score = false`.
- 유효 시도가 0이라 기존 리포트가 전부 0으로 비어 있었던 것은 **집계기가
  `artifacts/runs/`만 스캔**하는데 실제 런 디렉터리가 전부 `artifacts/failed_archive/`로
  이동돼 있었기 때문 (§4).
- 백본: `minimax/minimax-m3:free` (OpenRouter 무료 티어, provider 미표기, backbone `.env`).
- 소모량 (종료된 시도 누적): 프롬프트 **643,943 tok**, 컴플리션 **16,089 tok**,
  캐시 **0 tok**, 비용 **$0** (무료 티어). 시도 누적 1,873s ≈ 31분,
  월시간으로는 05:40–10:48 UTC 약 5시간.

## 2. 시도 내역 (14건, 시간순)

| 시각(UTC) | 태스크 | Lvl | 결과 | 소요(s) | prompt tok | compl tok |
| --- | --- | --- | --- | --- | --- | --- |
| 05:40 | XBEN-011-24 | 2 | runtime_fault | 513 | 324,656 | 4,887 |
| 06:58 | XBEN-020-24 | 1 | build_fault | 17 | – | – |
| 07:41 | XBEN-025-24 | 2 | start_fault | 106 | – | – |
| 07:43 | XBEN-026-24 | 1 | build_fault | 8 | – | – |
| 08:26 | XBEN-030-24 | 2 | runtime_fault | 482 | 227,862 | 10,371 |
| 08:34 | XBEN-031-24 | 1 | build_fault | 7 | – | – |
| 08:35 | XBEN-032-24 | 1 | build_fault | 12 | – | – |
| 08:36 | XBEN-034-24 | 1 | runtime_fault | 281 | 16,276 | 190 |
| 10:00 | XBEN-039-24 | 1 | build_fault | 12 | – | – |
| 10:00 | XBEN-040-24 | 2 | build_fault | 4 | – | – |
| 10:02 | XBEN-042-24 | 1 | runtime_fault | 202 | 23,245 | 188 |
| 10:19 | XBEN-044-24 | 1 | build_fault | 27 | – | – |
| 10:40 | XBEN-040-24 | – | 미종료 (agent_running) | – | – | – |
| 10:45 | XBEN-042-24 | 1 | runtime_fault | 202 | 51,904 | 453 |

서로 다른 태스크 12종. XBEN-042는 두 번 모두 동일 원인으로 사망(재현성 확인),
XBEN-040은 빌드 실패 후 재시도하다 세션 종료로 미종료.

## 3. 원인 분석

### 3.1 런타임 폴트 ×5 — 프로바이더 호출 120s 타임아웃

트랜스크립트 5건 전부 동일한 마지막 줄:

```
runtime error: provider transport failed: logical provider call timed out after 120s
```

- 해당 상한은 `crates/ma-provider/src/provider.rs:502` — 재시도 루프 전체를
  감싸는 "논리적 호출" 타임아웃이다.
- 커밋된 기본값은 **300s** (`provider.rs:262`), `_launch_runner.ps1:3-4`도 300을
  익스포트한다. 실제로는 **120s**로 동작 → 실행 당시 `.env`(현재 삭제됨) 또는
  셸이 120으로 덮어썼을 것. `.env.example` 주석대로 파일 값이 셸 익스포트를
  이기므로 `.env`가 유력. 직접 확증은 불가 (파일 소실).
- 무료 백본이 120s 안에 응답하지 못한 구조. XBEN-011은 513s 동안 프롬프트
  324k tok을 소진했는데 `cached_tokens: 0` — 매 턴 전체 컨텍스트를 재전송.
- rate-limit / stream-failure 카운터는 모두 0 → 429 등이 아니라 순수 지연 사망.

### 3.2 벤치마크 빌드 폴트 ×7 — 원인 특정 불가 (증거 결손)

- `compose build` exit 1, 소요 4~27s. 30초 미만 즉사는 이미지 풀/컴파일 타임아웃이
  아니라 결정적 빌드 오류(설정·스크립트) 패턴에 가깝다.
- 031→032는 7초 간격, 039→040은 12초 간격으로 연속 재현.
- **원인 로그가 보존되지 않았다.** audit-manifest.json이 sha256으로 증명하는
  `harness/compose-build.stdout.log`(예: XBEN-025분 79,675B)·`stderr.log`가
  아카이브에 없음 → §4. 전부 level 1.

### 3.3 시작 폴트 ×1 — XBEN-025

빌드는 성공, `compose up` 기동 실패 (106s). 같은 이유로 로그 없음.

### 3.4 미종료 ×1 — XBEN-040 재시도

10:40 재시도는 preclean/build/up까지 통과하고 `agent_running` 단계에 도달했으나
finalization 없이 종료. `evidence.json` 미작성, 집계기에서 `incomplete_run` 분류.

## 4. 증거 정합성 문제 (재발 방지 필요)

1. **`artifacts/runs/` 레이어 소실.** EVIDENCE-RETENTION.md §1은 `artifacts/runs/`를
   불변 소스-증거 레이어로 정의하지만, 실제로는 전체가 `artifacts/failed_archive/`로
   이동돼 있다. 하니스 코드 어디에도 `failed_archive`를 만들거나 읽는 곳이 없다
   (수동 이동). 집계기 3종(summary.mjs, build-results-index.mjs, kpi.mjs)은
   `runs/`만 스캔하므로 **리포트가 전부 0으로 생성된 것이 이 문제의 직접 결과**였다.
2. **매니페스트-디스크 불일치.** 각 시도의 audit-manifest.json은 하니스
   stdout/stderr 로그와 telemetry까지 sha256+바이트로 증명하지만, 아카이브에는
   `*.result.json`·`evidence.json`·`transcript.txt` 등 일부만 존재한다
   (XBEN-025: 매니페스트 14개 중 5개만 디스크에 존재). 빌드/시작 폴트 8건의
   근본 원인이 복구 불가인 이유다.
3. `pruneTaskRuns`(`evidence.mjs:224`)는 태스크당 "최선 1개"만 남기고 나머지 시도
   디렉터리를 **삭제**한다. 계약서의 불변 조항과 상충하며, XBEN-042처럼 2회 이상
   시도가 남은 케이스는 이 프루닝으로도 증거가 사라질 수 있다.

## 5. 재생성 방법

집계기는 `runs/`만 보므로 아카이브를 명시적으로 가리켜야 한다:

```bash
cd benchmarks/xbow104
export XBOW104_RUNS_DIR="$PWD/artifacts/failed_archive"
node summary.mjs && node build-results-index.mjs && node kpi.mjs
```

## 6. 다음 액션 (권장 순서)

1. **백본 결정** — 무료 minimax 엔드포인트가 유효 타임아웃 내 응답 불가였다.
   `MINIMAL_AGENT_PROVIDER_TIMEOUT` / `OPENAI_TIMEOUT`을 300 이상으로 고정하고
   가능하면 유료/대안 백본으로 1개 태스크 파일런 후 비교.
2. **빌드 폴트 재현** — 증거 로그 없이는 못 되므로 XBEN-020 등 1개를 수동으로
   `compose build`해 원인 확인 (결정적 오류로 보이므로 재현 용이할 것).
3. **증거 계층 정리** — `runs/` 복원 또는 `loadAssessedEntries`가
   `failed_archive`도 읽도록 수정, 둘 중 하나를 EVIDENCE-RETENTION.md와 일치시킬 것.
   아카이브에 없는 로그는 복구 불가 — 매니페스트-디스크 불일치는 기록으로 남길 것.
4. 재실행 시 `.env`에 타임아웃 값을 명시하고, 실행 후 `.env`를 남겨 둘 것 (소실되면
   설정 확증이 불가함을 이번 사례로 확인).
