# 수정·검증 상태

2026-09-05 작업 기록. [실행 안내](../README.md) · [분석·증거 기준](METHOD.md)

완료는 코드·로컬 회귀검증 기준이다. 실벤치마크 104개 재실행이나 새 이미지 배포로 정답률 향상을 확인한 기록은 아니다. 최초 감사의 정성 점수 6/10은 과거 평가이며 현재 성능 지표로 재사용하지 않는다.

## 수정한 문제

| 영역 | 원인 | 변경·확인 |
|---|---|---|
| 타깃 선택 (H-01) | compose의 첫 앱을 골라 XBEN-020 내부 서비스를 선택 | 공통 선택기가 공개 TCP 앱 우선. 서비스 순서·UDP·내부8080·누락 상태 테스트. 내부 포트 공개 패치는 하지 않음 |
| 사전 검사 | 값 없는 숫자 옵션이 true→1로 변환, suite 패치 실패 무시 | 제한값·실패 상태 검사 후 실행. 유료 호출 전 차단 테스트 |
| 자원 소유권 | 옛 감시자가 같은 과제의 새 시도를 정리할 수 있음 | wrapper token·PID·lock·정확한 run-dir 사용 |
| 종료·이미지 | 비동기 정리 전 결과 확정, 취소 후 배정, 공유 이미지 강제삭제 | 정리 완료 대기·취소 배정 중단·프로젝트 태그만 non-force 정리 |
| 네트워크 진단 | 존재하는 네트워크를 모두 orphan으로 경고 | 사용 중·빈 후보·조회 미확정 구분 |
| headless 증거 | 요청 완료 뒤 이벤트를 읽어 1,024개 방송 버퍼에서 ToolFinished 유실 | 요청과 수신 병행, 초기 호출부터 deadline. 이번 sequence 이후 journal ToolResult로 최종 판정 |
| journal 조회 | 큰 무관 blob과 반복 전체 조회가 비용 증가 | 종류·sequence 선택 visitor, 단일 순회·개별 크기 제한·checksum/digest 검사 |
| 통계 (H-03·H-06) | 회복한429까지 제외, 오래된 telemetry 경로, total=0, 캐시·분모 오류 | 최종 결과 중심 분류, usage.jsonl 인식, 합산·null·캐시 정규화, 유효 분자/분모 일치 |
| 재시도 보존 | 최고 성공만 남기는 정리가 실패·소비 이력을 삭제 | 자동 삭제 제거. 역할이 없어진 prune 호환 함수와 호출도 정리 |
| 빌드 경로 | 직접 Docker 빌드·오래된 app 태그 참조 | dimage와 runner→app→base 의존관계, Node24 래퍼·전체 Node 테스트 연결 |
| 중첩 worker | worker 주체로 main brief 갱신·실패 정리를 호출 | 런타임 상태 갱신 주체 정정. leaf 모델 호출과 실패 후 slot 회수 검증 |
| note 저장 | 크기 거부 전에 durable append | 기존 크기 검사 선행. 이전 note·sequence·recover 보존 검증 |
| scope 변경 | 검증 전에 기존 설정을 take해 실패 시 유실 | 검증→journal→메모리 순서. 다음 모델 요청의 기존 scope 보존 검증 |
| 재시작 안내 | root만 복원한다는 프롬프트가 A-light 구현과 불일치 | 살아 있던 worker를 main 직속으로 복원하는 실제 동작으로 안내 정정 |
| XSS 검증기 stdout 오염 (23개 과제) | check.js의 `print(status)`가 stdout 첫 줄에 "success\n"을 선행 출력하여 app.py의 `readline()`이 항상 success만 수신, alert('XSS') 발생과 무관하게 오답 처리 및 타임아웃 | `patch-suite.mjs`에 `patchCheckJs` 추가하여 `print(status)` 라인 제거. 23개 XSS 전 과제 정상 alert('XSS') 수신 보장 |
| 호스트 포트 하드코딩 충돌 (XBEN-094, 099) | `docker-compose.yml`의 `5003:5003`, `3000:3000` 고정 호스트 포트 지정으로 호스트 점유 프로세스와 충돌(`port is already allocated`) | `patch-suite.mjs`의 `patchCompose`에서 고정 포트를 동적 포트(`- "5003"`, `- "3000"`)로 변환하여 Docker 에페머럴 포트 자동 할당 보장 |
| Node 14 ES2021 문법 오류 (XBEN-084) | XBEN-084 s3 모의 서버의 `node:14-alpine` 환경에서 s3rver 전이 의존성(`@so-ric/colorspace`)이 ES2021 논리 할당 연산자(`||=`)를 사용하여 `SyntaxError` exit 1 크래시 | `patch-suite.mjs`의 `patchDockerfile`에서 `node:14-alpine`을 `node:18-alpine`으로 자동 업그레이드 |
| Tomcat 9 cgroup v2 NPE (XBEN-035) | `tomcat:9-jdk17-openjdk-slim`의 초기 OpenJDK 17 버그(JDK-8272270)로 cgroup v2 환경에서 NPE 발생 후 정상 종료(exit 0), 분리된 apt layer로 404 빌드 실패 | `patch-suite.mjs`에서 `tomcat:9-jre17-temurin`(cgroup v2 픽스 및 curl 내장)으로 교체 및 분리된 apt 레이어 통합 |

타깃·lifecycle·metrics는 실제 러너 함수에 fixture·mock을 적용했다. headless는 이벤트 폭주·미완료 호출·과거 flag·큰 무관 blob을 재현했다. 코어 note/scope/spawn/panic 회귀는 수정 전 실패와 수정 후 통과를 확인했다. suite 시로트 패치는 `patch-suite.mjs`를 통해 멱등성을 검증했다.

## 남은 문제와 설계상 한계

| 항목 | 상태·다음 판단 |
|---|---|
| Claude timeout 사용량 (H-02) | 최종 result 이벤트 전에 종료하면 usage/turns가 누락될 수 있음. null 표시 보정과 원본 복원은 다른 일이며 수집 개선은 미완료 |
| 분류의 러너 간 차이 (H-03) | 정상 재시도 오분류는 수정. runtime_fault 제외 정책과 대표 attempt 선택은 완전히 통일되지 않음 |
| MiniMax 무료 백본 지연 (H-04) | 특정 제공자·예산의 관찰. 타깃 결함으로 단정하지 말고 동일 조건으로 재측정 |
| TIMEOUT 문자열 오보 (H-05) | 설정 로그 문자열 대신 evidence의 최종 상태로 판단하도록 운영 절차 정리 |
| 시작 실패 진단 로그 (H-07) | 일부 compose 조회가 표준 harness artifact 캡처 밖에 있어 근거가 부족할 수 있음. 캡처 통합은 후속 작업 |
| 도구 원문 | journal 기록 전에 출력이 축약될 수 있음. 큰 증거는 파일로 보존. 원문 전량 보존을 보장하지 않음 |
| note와 compaction brief | 자유 note가 입력에 우선하고 압축은 구조화 brief 기준. 의미·예산 일치 보장은 미완료 |
| 압축 fallback | 계속 작업하기 위한 기계적 축약은 의도된 정책. coverage 검사는 의미 보존의 독립 증명이 아님 |
| panic 범위 | unwind는 정리하지만 프로세스 abort·강제 종료에는 destructor가 실행되지 않음 |
| 셸 자율성 | 별도 승인 엔진 부재는 의도된 설계. 다중사용자 보안 sandbox 보장과 다름 |

과거 GLM 59/98, 다른 스냅샷 59/99, Claude16/19, MiniMax2/2는 실행·선택 정책이 달라 직접 비교하지 않는다. 잘못 선택한 과거 XBEN-020은 성공으로 소급 인정하지 않으며 원본 유실도 이번 수정으로 복원되지 않는다.

## 검증

| 실제 실행 | 결과 |
|---|---|
| dbuild test --workspace --all-targets --quiet | 192 passed, 0 failed |
| dbuild fmt --all -- --check | exit 0 |
| dbuild clippy --workspace --all-targets -- -D warnings | exit 0 |
| nverify 기본 Docker Node24 gate | 50 passed, verify-project·pack 검사 통과 |
| PowerShell check·dimage·nverify 테스트 | 통과 |
| 경로 통합 뒤 Node gate·dimage 테스트 | 통과 |
| 문서·코드 정리 및 MiniMax 제거 후 Node gate | 42 passed, 0 failed. MiniMax 전용 8개 제거 |
| patch-suite 멱등성 및 verifier/port 패치 | dockerfiles=2, compose-ports=15, check-js=23 통과, 2차 실행 idempotent 0건 확인 |

```powershell
& ./scripts/dbuild.ps1 test --workspace --all-targets --quiet
$formatArgs = @('fmt', '--all', '--', '--check')
& ./scripts/dbuild.ps1 @formatArgs
$lintArgs = @('clippy', '--workspace', '--all-targets', '--', '-D', 'warnings')
& ./scripts/dbuild.ps1 @lintArgs
& ./scripts/nverify.ps1
```

라이브 보고서 재생성, 실제 provider·브라우저·TUI smoke, release 이미지 빌드는 이 검증에 포함되지 않는다. 새 Rust 코드 적용에는 라이브 종료 후 이미지 재빌드가 필요하다.

## 유지보수

상태가 바뀌면 이 표를 갱신한다. 같은 문제의 별도 감사·개선·검증 문서를 새로 만들지 않는다. 런타임 main projection 9곳과 brief byte 검사는 공통화했고, headless의 미사용 sequence 반환·추가 조회를 제거했다. 기존 journal resume에 쓰이는 호환 코드는 유지한다.

MiniMax 로컬 실행 코드는 사용자 요청으로 제거했다. 외부의 81/104 결과는 아직 가져오거나 검증하지 않았다.
