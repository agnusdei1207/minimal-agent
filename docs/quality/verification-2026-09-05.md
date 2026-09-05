# 최종 검증 — 2026-09-05

대상: C:\workspace\minimal-agent, main 작업 디렉터리의 이번 수정 완료 상태.
아래는 이 세션에서 실제 실행한 결과다. 이전 문서의 통과 주장을 재사용하지 않았다.

| 검증 | 결과 |
|---|---|
| dbuild test --workspace --all-targets --quiet | exit0, 188 passed / 0 failed / 0 ignored |
| dbuild fmt --all -- --check | exit0 |
| dbuild clippy --workspace --all-targets -- -D warnings | exit0, 경고 없음 |
| nverify 기본 Node24 Docker gate | exit0, Node tests50/50 + verify-project + npm pack dry-run + verify-pack |
| test/check.test.ps1 | exit0, 이미지 빌드 후 fresh isolated run 계약 |
| test/dimage.test.ps1 | exit0, default builder와 runner→app→base 빌드 인자 mock |
| test/nverify.test.ps1 | exit0, Docker default/Node24 host 검사/literal argv/잘못된 버전 거부 |
| git diff --check | exit0 |
| 관련 벤치마크 Markdown 상대링크 확인 | 통과 |
| 독립 코드 리뷰 | target/lifecycle/headless 교차검토 및 발견된 경계 수정 후 차단 결함 없음 |

재실행 명령(저장소 루트 PowerShell):

```powershell
& ./scripts/dbuild.ps1 test --workspace --all-targets --quiet
$formatArgs = @('fmt', '--all', '--', '--check')
& ./scripts/dbuild.ps1 @formatArgs
$lintArgs = @('clippy', '--workspace', '--all-targets', '--', '-D', 'warnings')
& ./scripts/dbuild.ps1 @lintArgs
& ./scripts/nverify.ps1
& ./test/check.test.ps1
& ./test/dimage.test.ps1
& ./test/nverify.test.ps1
```

PowerShell 인자 파싱에서 Cargo의 `--` 구분자가 유실되지 않도록 Rust 후행 인자는 배열로
전달했다. Node 테스트는 fixture/tempdir/mock으로 실제 러너 함수의 경계를 검증한다.
진행 중 타깃이나 유료 provider를 테스트 대상으로 호출하지 않는다.

추가 재현·검증:

- XBEN-020 공개앱 선택: compose 서비스 순서 양쪽, JSON 배열/NDJSON, 세 실제 discovery 함수.
- TCP/UDP, 내부8080, 실패/손상/누락 compose 결과, bare numeric 옵션 및 patch 실패.
- 정상 provider 재시도, 미측정 KPI, 재시도 소비·캐시·분모, 시간구간 문구.
- watchdog의 이전/현재 소유권, 취소 후 배정 중단, timeout 정리 순서, 모든 종료의 이미지 정리.
- headless 초기 호출 deadline, 방송 overflow, 과거 resumed flag 제외, 큰 무관 journal blob.
- journal watermark와 선택 visitor의 byte/sequence/checksum/blob 검증.

수행하지 않은 검증:

- 전체104개 실벤치 재실행 또는 XBEN-020 실타깃 재시도
- release 이미지 실제 빌드, 실제 브라우저/interactive TUI smoke
- 라이브 산출물 재생성 또는 과거 삭제 evidence 복원

현재 라이브 러너·컨테이너·외부 suite를 변경하지 않았고 새 벤치마크를 시작하지 않았다.
이미지 의존관계는 bake --print와 mock으로 검증했다. 새 Rust 바이너리의 실사용 반영은
공유 suite의 라이브 작업이 종료한 뒤 dimage로 이미지를 다시 빌드해야 한다.
커밋/푸시/새 브랜치/워크트리/PR을 만들지 않았다.

관련 문서:

- [프로젝트 품질 평가](project-audit-2026-09-05.md)
- [운영 런북](../../benchmarks/xbow104/RUNBOOK.md)
- [후속 에이전트 프롬프트](../../benchmarks/xbow104/HANDOFF.md)

관련 변경 설명: [벤치마크 신뢰성 개선 보고서 — 원인·변경 전후·검증](benchmark-improvements-2026-09-05.md)
