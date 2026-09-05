# XBOW-104 벤치마크 실행 가이드라인

이 문서는 XBOW-104 벤치마크의 **환경 세팅, 실행, 결과 집계, 트러블슈팅**을 한눈에 보고 따라 할 수 있는 원스톱 표준 가이드입니다.

---

## 1. 사전 준비 (Setup)

### Step 1. 벤치마크 스위트 클론 (최초 1회)
스위트 저장소를 `benchmarks/xbow-validation-benchmarks` 경로에 클론합니다:
```powershell
git clone https://github.com/xbow-engineering/validation-benchmarks benchmarks/xbow-validation-benchmarks
```

### Step 2. API 및 모델 설정 (`.env`)
프로젝트 루트 `.env`에 사용할 모델의 OpenAI 호환 API 정보를 설정합니다:
```dotenv
OPENAI_BASE_URL=https://api.z.ai/api/coding/paas/v4  # 사용할 API 엔드포인트 URL
OPENAI_API_KEY=your-api-key                         # API 키
OPENAI_MODEL=glm-5.3-flash                          # 모델 식별자
OPENAI_PROVIDER=zai                                 # 프로바이더명 (선택)
OPENAI_CONTEXT_TOKENS=262144                        # 컨텍스트 창(= 응답예약 상한, 아래 주의)
OPENAI_MAX_TOKENS=131072                            # 응답 토큰 상한 (아래 주의 참고)
MINIMAL_AGENT_PROVIDER_TIMEOUT=300                  # 응답 타임아웃 (초)
```
*(OpenRouter, z.ai, vLLM, Ollama 등 OpenAI 호환 규격을 지원하는 모든 서비스의 URL과 키를 그대로 입력하면 됩니다.)*

> **토큰 상한 — 두 변수를 함께 설정해야 합니다 (구조적 커플링).**
> - `OPENAI_MAX_TOKENS` = 프로바이더 `max_completion_tokens`(응답 최대 길이)이자 **컨텍스트에서 예약되는 응답분**. 낮으면 추론 백본이 사고 후 도구호출 JSON을 내보낼 때 중간 절단(`MalformedToolCall`, EOF 파싱)돼 과제 실패.
> - `OPENAI_CONTEXT_TOKENS` = 컨텍스트 창 상한(런타임 `configured_context_tokens`). **유효 입력 = `OPENAI_CONTEXT_TOKENS − OPENAI_MAX_TOKENS`** 이므로 반드시 **`CONTEXT > MAX`** 여야 하며, 아니면 예산이 음수가 돼 런타임이 기동하지 않습니다.
> - **모델 실제 상한(2026-09 조사)**: DeepSeek-V4 — 컨텍스트 1M, 최대 출력 **384K**; GLM-5.x — 컨텍스트 1M, 최대 출력 **131,072**. `OPENAI_MAX_TOKENS`는 해당 모델 최대 출력을 초과하면 프로바이더가 400으로 거부하므로 그 이내로 둡니다.
> - **권장(CTF 기준)**: `OPENAI_CONTEXT_TOKENS=262144`, `OPENAI_MAX_TOKENS=131072` (유효 입력 131K — CTF 프롬프트엔 충분, 출력 절단 없음). 런타임 기본값은 32768.
> - 런별 오버라이드: 러너 `--max-tokens <N>` (플래그 > `.env` > 기본값). 시작 배너 `max-tokens N`으로 실제 적용값 확인.

### Step 3. 스위트 자동 보정 패치 (필수)
시간 경과에 따른 Docker 빌드 실패(Debian Bullseye/EOL 저장소 404, XSS 판정기 stdout 오염, 고정 포트 충돌 등)를 자동 보정합니다:
```powershell
& ./scripts/nverify.ps1 -HostNode -NodeArguments @('benchmarks/harness/patch-suite.mjs')
```
- 1차 실행 시 패치 건수가 출력되고, 즉시 재실행 시 **모든 항목이 0건(멱등성)**이어야 합니다.

### Step 4. 에이전트 러너 이미지 빌드
에이전트 런타임 이미지를 최신 로컬 워킹트리 소스 기반으로 빌드합니다:
```powershell
powershell -ExecutionPolicy Bypass -File scripts/dimage.ps1 -Target runner -Tag xbow-agent-runner:latest
```

---

## 2. 벤치마크 실시 (Execution)

모든 러너 명령은 Node 24 환경을 보장하는 `scripts/nverify.ps1`을 통해 실행합니다.

### ① 단일 과제 디버깅 실행
특정 과제 1개만 빠르게 테스트할 때 사용합니다:
```powershell
& ./scripts/nverify.ps1 -HostNode -NodeArguments @('benchmarks/harness/task.mjs', 'XBEN-001-24', '--no-commit')
```

### ② 특정 과제군 선별 배치 실행
원하는 과제 목록만 지정하여 병렬 실행합니다:
```powershell
& ./scripts/nverify.ps1 -HostNode -NodeArguments @(
  'benchmarks/harness/runner.mjs',
  '--tasks', 'XBEN-001-24,XBEN-002-24,XBEN-003-24',
  '--concurrency', '5',
  '--timeout', '900',
  '--no-commit'
)
```

### ③ 104개 전체 과제 실행
스위트 전체 과제를 순차 실행합니다:
```powershell
& ./scripts/nverify.ps1 -HostNode -NodeArguments @(
  'benchmarks/harness/runner.mjs',
  '--all',
  '--concurrency', '5',
  '--timeout', '900',
  '--no-commit'
)
```

### ④ 모델별 분리 실행 (아티팩트 및 컨테이너 격리)
기본 GLM 외의 다른 모델(예: DeepSeek)을 분리하여 실행할 때 사용합니다:
```powershell
$env:XBOW104_ARTIFACTS_DIR = "benchmarks/deepseek-v4-flash/artifacts"
$env:XBOW104_PROJECT_PREFIX = "dsv4-"
& ./scripts/nverify.ps1 -HostNode -NodeArguments @('benchmarks/harness/runner.mjs', '--all', '--concurrency', '5', '--timeout', '900', '--no-commit')
```

---

## 3. 핵심 운영 수칙 (Rules)

1. **동시성 5 이하 엄수 (`--concurrency 5`)**:
   - 5를 초과하면 z.ai 429 레이트리밋(code 1302)이 다발하며, Docker 브리지 네트워크 및 호스트 시스템 자원 고갈이 발생합니다.
2. **`--no-commit` 플래그 사용**:
   - 사용자의 명시적인 커밋 승인이 없는 디버깅 및 반복 런에서는 항상 `--no-commit`을 명시합니다.
3. **환경 도구 규격**:
   - Node 24 실행은 항상 `scripts/nverify.ps1`을 경유합니다.
   - Rust 빌드/테스트는 `scripts/dbuild.ps1`을 사용하며, 호스트 Cargo는 절대 직접 실행하지 않습니다.
4. **상시 런 모니터링**:
   - 장시간 실행은 5분 간격 모니터링 서브에이전트로 위임하고, 새로운 장애·이슈 발생 시에만 보고합니다.

---

## 3.5 무한 사고·토큰 낭비 감지 및 강제 중지 (Runaway Reasoning)

추론 백본은 도구 호출 없이 "생각"만 반복하며 응답 토큰을 소진(진전 없는 루프)할 수 있습니다. 이는 정상 실패와 달리 시간·비용만 낭비하므로 조기에 감지해 강제 중지합니다.

### 감지 신호 (트랜스크립트/텔레메트리 기반)
런 디렉터리 `<artifacts>/runs/<task>-<stamp>/` 에서 확인합니다:
- **텔레메트리** `telemetry/usage.jsonl`: `completion_tokens`가 여러 턴 연속으로 상한(`OPENAI_MAX_TOKENS`) 부근에 붙어 있고, 새 도구 결과 없이 누적만 증가 → 사고 루프.
- **저널** `ma-run/journal/*.jsonl`: `Reasoning`/`Transcript` 이벤트만 다수이고 `ToolCall`·`ToolResult`가 장시간 나오지 않음 → 진전 없는 사고.
- **트랜스크립트** `transcript.txt`: 컨테이너는 살아 있으나 새 도구 활동(요청/결과)이 멈춤.

```powershell
# 특정 런의 최근 completion_tokens 추세(상한 근처 반복 여부) 확인
Get-Content "<run>/telemetry/usage.jsonl" -Tail 8 | ForEach-Object { ($_ | ConvertFrom-Json).completion_tokens }
# 저널에서 도구 호출이 최근 발생했는지 확인 (없으면 사고 루프 의심)
Select-String -Path "<run>/ma-run/journal/*.jsonl" -Pattern '"ToolCall"|"ToolResult"' | Select-Object -Last 3
```

### 강제 중지
- **수동적(기본)**: 러너 `--timeout`이 데드라인에서 에이전트 컨테이너를 `docker kill`로 강제 종료하므로, 상한 시간(예: 900~1800초)이 무한 루프의 하드 실링입니다. 타임아웃을 과도하게 크게 잡지 않습니다.
- **능동적(조기 중지)**: 위 신호로 루프가 확인되면 데드라인 전이라도 해당 과제 에이전트 컨테이너만 강제 종료합니다. 러너는 이를 미해결/결함으로 기록하고 다음 과제로 진행합니다:
  ```powershell
  # 에이전트 컨테이너명 = "<PREFIX>xben-<id>-24-agent" (예: dsv4-xben-002-24-agent)
  docker kill <PREFIX>xben-<id>-24-agent
  ```
- **근본 완화**: `OPENAI_MAX_TOKENS`를 추론 여유가 있는 값(권장 32768)으로 설정하면 도구 호출 truncation은 사라지되, 상한 자체가 턴당 사고량의 천장이 되어 무한 사고를 방지합니다.

---

## 4. 결과 집계 및 리포트 (Reporting)

러너가 완료되면 공통 리포터로 표준 분석 문서를 갱신합니다:
```powershell
# GLM-5.3-Flash 결과 집계
node benchmarks/zai/summarize.mjs --model glm-5.3-flash

# DeepSeek-V4-Flash 결과 집계
node benchmarks/zai/summarize.mjs --model deepseek-v4-flash
```
- **생성 위치**: `<artifacts>/reports/`
  - `SUMMARY.md`: 정답률, 토큰 소모량, 시간 등 요약 보고서
  - `kpi.json`: 머신 리더블 지표 데이터
  - `results-index.json`: 과제별 최종 판정 및 제외 목록

---

## 5. 트러블슈팅 및 비상 조치 (Troubleshooting)

| 문제 상황 | 주원인 | 해결 조치 |
| :--- | :--- | :--- |
| **빌드 실패 (`benchmark_build_fault`)** | Bullseye 보안 미러 CDN 404 또는 EOL 저장소 미러링 결함 | `& ./scripts/nverify.ps1 -HostNode -NodeArguments @('benchmarks/harness/patch-suite.mjs')` 실행 후 재빌드 |
| **기동 실패 (`benchmark_start_fault`)** | 이전 실행 잔여 컨테이너가 포트를 점유 (`port is already allocated`) | 잔여 컨테이너 일괄 정리:<br>`docker rm -f $(docker ps -aq --filter name=xben)` |
| **긴급 중단 (Cancel)** | 런 도중 즉시 중단이 필요한 경우 | 터미널에서 `Ctrl+C` 입력 후 잔여 스택 정리:<br>`docker rm -f $(docker ps -aq --filter name=xben)` |
| **Docker 빌더 에러** | Docker 빌더 컨텍스트가 `default`가 아님 | 기본 빌더 컨텍스트로 전환:<br>`docker --context=default buildx use default` |
