# XBOW-104 운영 런북 (standing procedure)

이 문서는 XBOW-104 벤치를 돌릴 때 **매번 반복 지시하지 않아도** 지켜야 하는
표준 절차다. 사람이든 다른 모델이든 이 벤치를 수행하면 아래를 그대로 따른다.

## 0. 사전 (필수, 순서대로)
1. `.env` 백본 확인 — 런타임은 `OPENAI_*`만 읽는다(`ANTHROPIC_*` 무시).
   z.ai: `OPENAI_BASE_URL=https://api.z.ai/api/coding/paas/v4`,
   `OPENAI_MODEL=glm-5.3-flash`. 자세한 백본 함정은 [[xbow104-run-state]] 메모리.
2. **`node benchmarks/xbow104/patch-suite.mjs`** — 스위트 빌드 시로트 수정(idempotent).
   원인·상세는 [BUILD-INFRA.md](BUILD-INFRA.md).
3. `npm run xbow:doctor` — 백본/도커/이미지 진단.

## 1. 실행
```bash
node benchmarks/xbow104/runner.mjs --all --concurrency 5 --timeout 900
```
- **동시성은 5 이하.** z.ai coding tier는 10에서 429(`code 1302`)가 나고
  RUNTIME_FAULT로 태스크가 오염된다. tokenrouter 무료(`z-ai/glm-5.3-free`)는
  분당 8콜 하드리밋이라 병렬과 상성이 나쁘다.
- 러너는 태스크마다 **리포트 재생성 → `git add/commit/push`** 하므로 커밋이
  누적된다(결과 누락 방지). `--no-commit`은 결과를 커밋하지 않으니 상시 런에는 쓰지 말 것.
- 러너는 태스크마다 `compose down -v`(볼륨 정리) + 해당 태스크 target 이미지
  삭제 + dangling prune 으로 **용량을 확보하며 진행**한다.
  (`XBOW104_KEEP_IMAGES=1` 로 끌 수 있음.)
- `--all`은 완료 태스크를 건너뛰고 미완료(빌드폴트 포함)만 재시도한다.
  이미 클리어한 문제는 자동 스킵되므로 굳이 `--rerun-all` 하지 말 것(플래키 뒤집힘 위험).

## 2. 모니터링 (5분 간격, 서브에이전트로 위임)
상시 런 동안 **5분마다 모니터링 서브에이전트(general-purpose) 1개**를 띄워
점검시키고, 결과에서 **이슈만 사용자에게 보고**한다(정상이면 조용히 noop).
직접 점검하지 말 것 — 위임해야 누락이 안 생긴다. 세션 크론(`*/5 * * * *`)이
매 발화마다 새 서브에이전트를 배치하고, 서브에이전트는 끝나면 종료된다(재배치는 크론이).

서브에이전트 체크리스트:
1. 로그 tail — 진행 태스크, SOLVED/UNSOLVED/BUILD FAILED/RUNTIME_FAULT 집계.
2. rate-limit/인증 스캔 — `grep -icE '1302|Rate limit reached|429|too many requests|quota|overload|401|403|unauthorized'`.
3. 실행 중 태스크 생존 — `artifacts/runs/<TASK>-*/telemetry/usage.jsonl` 증가 여부,
   스톨(마지막 응답 200s+ 이며 900s 근접) 여부.
4. 커밋 누적 — `git log --oneline`에 `bench(...)` 커밋이 쌓이는지, push 경고 유무.
5. 디스크 — `docker system df`로 이미지 사용량 폭증 없는지(태스크별 정리 작동 확인).
6. 완료 여부 — runner.mjs 프로세스 생존 + 로그 끝 신호.

보고 형식: 한 줄 상태(HEALTHY/ISSUE:…) + 집계 + 남은 BUILD FAILED 태스크·원인
+ 429수 + 최신 커밋 + 디스크 + 권고. **관찰·보고만**(파일수정/커밋/재기동 금지).

## 3. 에스컬레이션 (서브에이전트 보고 → 메인이 조치)
- **심각한 429**(여러 태스크 반복 + RUNTIME_FAULT 유발): 러너 정지 →
  `docker rm -f $(docker ps -aq --filter name=xben)` 정리 →
  concurrency 한 단계 하향(5→3→2→1) 재기동 → 크론을 새 런ID/로그로 갱신.
- **패치로도 안 고쳐진 새 빌드 실패**: 원인 분류 후 `patch-suite.mjs` 보강,
  재적용, 해당 태스크 재시도. (원인 4종 분류는 BUILD-INFRA.md)
- **완료 감지**: `npm run xbow:kpi && npm run xbow:summary`로 최종 리포트 갱신,
  SOLVED/UNSOLVED/제외 집계 요약 보고, 모니터링 크론 CronDelete.

## 4. 용량 관리
- 상시: 러너의 태스크별 정리에 맡긴다(위 1번).
- 수동 정리는 **미사용만**:
  `docker image prune -f`, `docker volume prune -f`, `docker builder prune -f`.
- **금지**: 라이브 런 중 `docker system prune -a --volumes` / 태그 이미지 강제삭제 —
  남은 태스크가 쓰는 베이스 이미지를 지우고 in-flight 빌드를 깨뜨린다.
- 오래된 `xben-*` target 이미지는 **런이 없을 때만** 일괄 삭제 가능:
  `docker images --filter reference='xben-*' -q | xargs docker rmi -f`.

## 5. 커밋/원격
- 커밋 아이덴티티 `agnusdei1207`. 러너가 자동 커밋/푸시.
- 원격이 force-push로 발산했었다면(2026-09-04 이력) 로컬이 정본. push 충돌 시
  사용자 지시에 따라 처리(로컬 우선 시 `--force-with-lease`).

## 6. 남는 제외
mirror/image 로트를 넘어 upstream 소스 자체가 사라진 소수 챌린지는 계속
`excluded_attempts`에 남는다 — 에이전트 실패가 아니라 정직한 제외다.
