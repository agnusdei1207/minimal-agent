# XBOW-104 운영 런북

XBOW/native GLM, MiniMax, Claude 비교 실행의 공통 기준이다. 장애를 추측으로 고치거나
같은 경고를 반복 보고하지 않는다. 상세 원인은 [BUILD-INFRA.md](BUILD-INFRA.md),
후속 에이전트 지시는 [HANDOFF.md](HANDOFF.md), 보존은 [EVIDENCE-RETENTION.md](EVIDENCE-RETENTION.md)를 따른다.

## 1. 실행 전

1. AGENTS.md를 읽는다. main에서 작업하고 기존 사용자 변경을 보존한다.
2. 현재 runner PID, task/run ID, compose project, 모델, 아티팩트 경로를 확인한다.
   이미 실행 중인 프로세스는 새 코드를 자동 로드하지 않는다. 중복 러너를 시작하지 않는다.
3. 자격증명 값은 출력하지 않는다. native XBOW 러너는 저장소 루트 `.env`의 OPENAI_*를,
   MiniMax 러너는 `benchmarks/minimax/.env`를, Claude 호스트 러너는 해당 CLI 인증을 사용한다.
   `npm run check`의 로컬 자격증명은 AGENTS.md대로 저장소 루트 `.env`에만 둔다.
4. 공유 suite를 쓰는 실행과 빌드가 끝난 뒤 patch-suite를 적용한다. 멱등 패치라도
   빌드 중인 Dockerfile/compose를 수정하면 안전하지 않다.
5. doctor의 WARN은 아래 분류표로 해석한다. WARN 하나만으로 장애를 보고하지 않는다.

모든 Node 실행은 저장소 래퍼를 거친다. 기본은 capped Docker 검증이며,
호스트 Docker CLI/Claude 인증이 필요한 하니스에만 Node 24 검사 후 실행하는 -HostNode를 쓴다.

```powershell
# 저장소 루트에서. 공유 suite 사용 중인 라이브 실행이 없을 때 수행한다.
& ./scripts/nverify.ps1 -HostNode -NodeArguments @('benchmarks/xbow104/patch-suite.mjs')
& ./scripts/nverify.ps1 -HostNode -NodeArguments @('benchmarks/xbow104/doctor.mjs')

# 필요 시 owned base → app → runner 빌드. 라이브 종료 후 실행한다.
& ./scripts/dimage.ps1 -Target runner -Tag xbow-agent-runner:latest

# 격리된 회귀검증. 실제 타깃·provider 호출 없이 수행한다.
& ./scripts/nverify.ps1
```

Node 기본 Docker 모드에는 호스트 Docker CLI/Claude 세션이 없으므로 실제 하니스를 그 모드로
시작하지 않는다. 애플리케이션/러너 이미지는 dimage.ps1로만 빌드한다.

## 2. 실행과 재개

동시성은 **전체 벤치마크 활성 태스크 합계 5 이하**로 운영한다. 각 러너도 1~5를 검사하지만
서로 다른 러너 사이의 합계는 운영자가 확인한다. provider 제한이 더 작으면 그 제한을 따른다.
timeout은 양의 유한 초 단위 값이다.

```powershell
# native GLM 등: 기존 유효 완료를 건너뛰고 이어서 실행
& ./scripts/nverify.ps1 -HostNode -NodeArguments @(
  'benchmarks/xbow104/runner.mjs', '--all', '--concurrency', '1',
  '--timeout', '900', '--no-commit'
)

# Claude XBEN-020의 잘못된 타깃 선택으로 제외된 시도만 재시도
& ./scripts/nverify.ps1 -HostNode -NodeArguments @(
  'benchmarks/claude/run.mjs', '--model', 'opus-4.8', '--tasks', 'XBEN-020-24',
  '--concurrency', '1', '--timeout', '900', '--no-commit'
)
```

- `--all`의 생략 기준은 러너별 기존 유효 결과다. timeout/unsolved도 완료된 시도일 수 있다.
  다시 측정할 때는 해당 task만 지정하고 필요한 경우에만 `--rerun-all`을 쓴다.
- **XBEN-020 진입점은 ssrf-demo-app**이다. internal-service를 직접 겨냥한 이전 GLM/MiniMax
  결과는 올바른 진입점 성능의 증거로 쓰지 않는다. 원본을 보존하고 해당 task만 새로 측정한다.
  내부 expose를 ports로 바꾸지 않는다.
- provider 러너는 시작 시 patch-suite를 자동 실행하므로 공유 suite가 유휴인지 먼저 확인한다.
- 이번 세션에 커밋/푸시 명시 승인이 없으면 **--no-commit 필수**다. 승인된 상시 런에서는
  태스크별 보고서 커밋 누적을 유지하고 Git identity는 agnusdei1207을 쓴다.
  push 충돌을 무단 rebase/force-push로 해결하지 않는다.
- 실행 중 코드를 교체해도 해당 프로세스에는 적용되지 않는다. 종료 후 새 프로세스로 적용한다.
  하니스/이미지 버전이 바뀐 전후 결과를 구분한다.

## 3. 장애 분류

| 관찰 | 확인할 증거 | 처리 |
|---|---|---|
| no published port | compose 앱 목록, ps JSON의 Service/Publishers, 선택 service | 공개 진입앱을 선택한다. 내부 서비스 공개 금지. 실제 공개 진입점 부재일 때만 start fault |
| active xben network | 연결된 컨테이너와 run ID | 정상 리소스. 삭제·장애 보고하지 않음 |
| empty network / ownership unknown | owner project/PID, build/start 구간 | 빈 네트워크만으로 고아 확정 금지. query 실패는 진단 미완료 |
| 429 후 응답 증가 | attempt provider 로그와 최종 exit/outcome | 일시 재시도. 정상 종료/timeout을 429 한 줄 때문에 제외하지 않음 |
| 401/403/402, 지속 quota fault | 응답 코드와 반복 구간 | provider/auth 문제. 타깃 인프라나 추론 실패로 부르지 않음 |
| solver timeout / unsolved | 타깃 정상 시작, solver 실행, 제한시간 | 유효 미해결. 근거 없이 infra로 제외하지 않음 |
| 지표 null 또는 - | 수집 파일 존재와 스키마 | 미측정. 0건·무결함·안전성 증거로 해석하지 않음 |
| 사용량 잠시 미증가 | provider deadline, 장시간 도구, 컨테이너, 두 번의 표본 | 200초 정체 하나로 스톨 확정 금지 |
| EOL mirror/phantomjs/composer 실패 | harness build stderr 최초 오류 | BUILD-INFRA 기존 패치 대상인지 확인. 동일 패치 무한반복 금지 |
| 기존 runner/lock | owner PID, run ID, finalized 여부 | 진행 중이면 기다림. 최신 task 이름만으로 다른 attempt 정리 금지 |
| 사용자 취소 | signal, 해당 run-state, runner PID | interrupted로 보존. 신규 task 배정 없이 해당 실행만 정리 |

증거가 부족하면 **원인 미확정**으로 적는다. runtime_fault가 기존 정책상 채점 제외여도
곧바로 타깃 INFRA는 아니다. 런타임 버그·하니스·provider 원인을 구분하고 attempted와
종류별 제외 수를 함께 남긴다. 한 백본의 오류만 보고 모든 백본이 실패한다고 추정하지 않는다.

## 4. 5분 모니터링 — 정상은 보고하지 않음

상시 런은 모니터링 서브에이전트에 5분 간격 점검을 위임한다. 관찰만 수행하고 파일수정,
컨테이너 조작, 재시작, 커밋은 하지 않는다.

1. 새로 종료된 attempt, 현재 task/run ID와 단계, 남은 작업을 확인한다.
2. 오류는 문자열 개수 대신 해당 attempt의 최종 영향으로 판정한다.
3. 사용량/로그의 이전 표본과 현재 표본을 비교한다. provider별 경로 차이를 고려한다.
4. runner/컨테이너 생존, 디스크 증가, 해당 태스크 정리 상태를 확인한다.
5. 커밋 승인 런에서만 커밋 누적/push를 확인한다. --no-commit은 결함이 아니다.

정상 진행, 단발 429, 예상 timeout, 이미 보고했고 상태 변화가 없는 같은 오류는 보고하지 않는다.
**새 차단 문제, 지속 악화, 데이터 손실 위험**만 다음 형식으로 한 번 보고한다.

```text
ISSUE [model/task/run-id] 단계: … / 최초 오류: … / 근거 파일: …
실제 영향: … / 기존 절차 적용 결과: … / 다음 최소 조치: …
```

중복 키는 모델+task+실패단계+오류서명이다. 상태가 변하거나 새 조치가 필요할 때만 재보고한다.

## 5. 정리와 결과 보존

- 정리는 러너의 해당 run ID에 속한 컨테이너/project/태스크 이미지에 맡긴다.
  감시기는 실행 소유권 일치와 runner 종료를 확인한 경우에만 보완 정리한다.
- 라이브 중 Docker 엔진 재시작, docker system prune -a --volumes, global builder/volume prune,
  이름 wildcard로 docker rm -f, 공용 베이스 이미지 강제삭제를 금지한다.
- 디스크가 부족하면 신규 배정을 줄이고 정확히 소유한 종료 리소스만 정리한다.
  실패 transcript/evidence는 삭제 대상이 아니다.
- 재시도는 새 attempt다. 이전 성공/실패/중단 증거를 삭제해 최고 결과만 남기지 않는다.
- XBOW 공통 점수는 최신 유효 attempt, provider summarize 점수는 최신 완료 attempt를 선택한다.
  선택한 attempt를 표시하고 전체 attempts 소비량과 별도로 집계한다.
  과제 수·하니스·시간·캐시·제외 기준이 다르면 성공률만으로 모델 순위를 만들지 않는다.
- 기존 duration_s는 빌드 전부터 측정한 attempt 경과시간이다. teardown 포함 여부도
  하니스마다 달라 solver 전용시간이나 모델 턴 지연시간으로 해석하지 않는다.
- 조사자가 라이브 보고서를 재생성하지 않는다. 실행 소유자가 완료 경계에서 provider별
  summarize/공통 보고서를 갱신한다. 손상된 raw 증거는 원위치 수정하지 않는다.
