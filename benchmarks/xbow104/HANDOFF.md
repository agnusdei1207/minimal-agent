# 벤치마크 후속 에이전트 프롬프트

아래 블록을 후속 에이전트에 그대로 전달한다. 이 문서의 명령은 실행 승인과 현재 리소스
소유권을 확인한 뒤 그 에이전트가 수행할 절차이며, 작성 과정에서 실행한 기록이 아니다.

```text
C:\workspace\minimal-agent 벤치마크를 기존 진행 상태에서 이어서 수행하세요.
먼저 AGENTS.md, benchmarks/xbow104/RUNBOOK.md, BUILD-INFRA.md,
EVIDENCE-RETENTION.md를 읽으세요. main 직접 작업이며 기존 미커밋 변경을 보존하세요.

1. 라이브 무간섭
- 현재 model별 runner PID, task/run ID, compose project, artifacts 경로를 확인하세요.
- 기존 러너가 돌고 있으면 새 러너/중복 태스크를 실행하지 마세요.
- 공유 suite의 라이브 작업/빌드가 모두 끝날 때까지 patch-suite 적용, 이미지 재빌드,
  태그 교체, 러너 재시작을 보류하세요. 진행 중 프로세스는 변경 코드를 hot-reload하지 않습니다.
- 전역 prune, 이름 wildcard 삭제, Docker 재시작, 공용 이미지 강제삭제를 하지 마세요.

2. 수정된 경로 사용
- Node 24는 scripts/nverify.ps1로만 실행합니다. 하니스에는 -HostNode와 -NodeArguments를
  사용하세요. 기본 Docker 검증 모드에는 host Docker CLI/Claude 인증이 없습니다.
- 유휴 시 patch-suite → doctor 순서로 점검하세요. native 런타임 변경을 반영할 필요가 있으면
  scripts/dimage.ps1 -Target runner -Tag xbow-agent-runner:latest로 base/app/runner를 빌드하세요.
- credentials는 native XBOW=루트 .env, MiniMax=benchmarks/minimax/.env,
  Claude=기존 CLI 인증에서 읽고 비밀값을 출력하지 마세요. check의 자격증명은 루트 .env만 씁니다.
- 전체 활성 벤치 태스크 합계 <=5, 처음 재시도는 concurrency1, timeout900초를 쓰세요.
- 이번 세션에 별도 커밋/푸시 명시 승인이 없으면 모든 러너에 --no-commit을 붙이세요.

3. XBEN-020 재평가
- 이 문제는 private service의 포트를 공개해야 하는 문제가 아니었습니다.
- 올바른 진입점은 ssrf-demo-app, internal-service는 의도된 SSRF 내부 대상입니다.
- shared target.mjs와 세 runner의 published-app 우선 선택을 사용하세요.
- 내부 expose를 ports로 바꾸거나 expected flag를 solver에 전달하지 마세요.
- Claude에서 잘못 선택되어 제외된 XBEN-020만 우선 재시도하세요.
- native GLM/MiniMax의 과거 XBEN-020 증거가 internal-service를 직접 가리켰다면,
  그 결과를 올바른 진입점의 점수로 간주하지 말고 보존 후 해당 task만 --rerun-all로 다시 측정하세요.
- 다른 정상 완료 태스크의 무조건 전체 재실행은 하지 마세요.

4. 증거와 통계
- 성공/실패/중단 attempt 원본을 삭제하거나 덮어쓰지 마세요. retry는 새 run ID입니다.
- 통계는 선택 attempt 식별자, attempted/scored/solved/제외사유를 함께 표시하세요.
- 공통 XBOW 집계는 최신 유효 attempt, provider summarize는 최신 완료 attempt를 선택합니다.
  비교 시 같은 정책을 선택한 자료끼리 사용하세요. 전체 소비량에는 보존된 재시도를 포함하세요.
- null/-는 미측정입니다. 안전신호0, 도구오류0, 토큰0의 증거로 해석하지 마세요.
- duration_s는 setup을 포함한 attempt 경과시간이며 teardown 포함 여부는 하니스마다 다릅니다.
  solver 전용시간이나 모델 latency로 보고하지 마세요.
- 과거 generator 결과는 자동으로 교정되지 않습니다. 실행 소유자가 완료 경계에서
  provider별 summarize 및 필요한 공통 집계를 재생성하세요. 다른 라이브 보고서를 수정하지 마세요.

5. 조용한 모니터링
- 상시 런은 5분 간격 관찰 전용 서브에이전트에 맡기세요.
- 정상 진행, 단발429 후 회복, 예상timeout, 기존에 보고한 변화없는 경고는 사용자에게 보내지 마세요.
- 새 차단 문제/지속 악화/데이터 손실 위험만 1회 보고하세요.
- 보고 전에 RUNBOOK 분류표와 harness 오류 파일을 확인하세요. 원인 부족은 원인 미확정입니다.
- ISSUE [model/task/run-id] 단계 / 최초오류 / 증거파일 / 실제영향 / 적용한 절차 / 다음최소조치
  형식으로 보고하고, 같은 모델+task+단계+오류서명을 중복 보고하지 마세요.
```

실행 예시(공유 suite가 유휴이고 중복 실행이 없을 때):

```powershell
& ./scripts/nverify.ps1 -HostNode -NodeArguments @('benchmarks/xbow104/patch-suite.mjs')
& ./scripts/nverify.ps1 -HostNode -NodeArguments @('benchmarks/xbow104/doctor.mjs')
& ./scripts/nverify.ps1 -HostNode -NodeArguments @(
  'benchmarks/claude/run.mjs', '--model', 'opus-4.8', '--tasks', 'XBEN-020-24',
  '--concurrency', '1', '--timeout', '900', '--no-commit'
)
```
