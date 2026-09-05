# 프로젝트 품질 평가 — 2026-09-05

평가 대상은 main의 현재 작업 디렉터리다. 조사 시작 HEAD는 7d6ce56이며, 기존 미커밋 코드와
라이브 벤치마크 산출물이 있었다. 아래 평가는 저장소 전체 구조·설계·핵심 실행 경로·검증 도구·
벤치마크 하니스의 조사다. 모든 실행 분기나 실제 104개 과제를 재검증했다는 의미는 아니다.
외부 유료 API 호출, 라이브 실행 변경, 커밋/푸시는 하지 않았다.

## 평가

**전체 6/10: 연구·실험용 기반은 갖췄지만, 에이전트 의미 보존과 실제 계층 실행에 중요한 공백이 있다.**
점수는 이 조사자의 정성 평가이며 외부 인증/벤치마크 점수가 아니다.

| 영역 | 점수 | 근거 |
|---|---:|---|
| 모듈 구조·코드 명료성 | 7/10 | core/provider/journal/coordinator/context/runtime/TUI의 7개 crate 경계가 명확. 다만 runtime 단일파일이 2천 줄 이상으로 여러 수명주기를 담당 |
| 장애·자원 경계 | 7/10 | journal checksum/blob/단일writer, 원자적 메시지 enqueue, 제한·취소·provider retry 계약 테스트가 실질적 |
| 팀 에이전트 실행 완성도 | 5/10 | main→worker 동작은 검증되어 있으나 실제 worker→grandchild 시작이 실패하고 live slot이 남는 문제 재현 |
| 장기 기억·증거 보존 | 5/10 | 원장과 brief 분리는 좋지만 출력 선절단, 기계적 문맥 제거, note/coverage brief 간 불일치 존재 |
| 테스트·검증 파이프라인 | 7/10 | 조사 시작 Rust 178개 통과. Node gate의 오래된 계약과 벤치 회귀검증 미연결을 이번에 수정 |
| 벤치마크 측정 신뢰성 | 6/10 | 타깃·통계·소유권 결함을 수정했으나 이전 잘못된 타깃 결과와 삭제된 증거는 새 코드로 복원되지 않음 |
| 문서·운영 일관성 | 6/10 | 이번 RUNBOOK/인수인계는 정리. 코어 ADR/프롬프트/README의 서로 다른 세대 계약은 추가 정합화 필요 |

조사 시점 규모: Rust 소스 23개 파일 약 12,018줄, integration test 파일 11개.
이 수치에는 작업 디렉터리에 있던 미연결 clipboard.rs도 포함된다. 테스트 개수는 실행 결과와
구분해야 한다. 코드 줄 수나 테스트 수 자체로 에이전트의 문제 해결력을 판정하지 않았다.

## 중요한 잔여 코어 문제

### P1 — 3단계 팀이 코디네이터에서는 생성되지만 런타임에서 시작되지 않음

`crates/ma-runtime/src/runtime.rs`의 spawn_worker는 caller로 sync_main_runtime을 호출한다.
직접 부모가 worker이면 main 전용 brief ownership 검사에서 실패한다. 실패 정리도 그 worker를
actor로 mark_terminal 호출하여 main 전용 권한에 걸리고 오류를 무시한다.

실제 재현: main→lead 성공 후 lead→leaf 호출은 `agent worker-01 cannot curate main's brief`로
실패했고 live_team은 3명으로 남았다. 실행되지 않는 자식이 permit을 차지한다.
코디네이터 수준의 depth2 테스트만으로 실제 계층 실행을 보장할 수 없음을 보여준다.
관련 위치: runtime.rs spawn_worker/start_worker_driver, ma-context/src/brief.rs sync_main_runtime,
ma-coordinator/src/lib.rs mark_terminal. 이 코어 동작은 이번 벤치마크 인프라 수정과 별개로 남겨두었다.

### P1 — note 쓰기 실패가 원장에 먼저 남아 재시작을 막을 수 있음

`ma-context/src/brief.rs` write_note는 BriefNote를 journal에 append한 뒤 write_projection에서
byte 한도를 검사한다. 한도를 넘는 note는 도구에서 실패해도 이미 durable 이벤트가 된다.
recover_matching은 그 note를 다시 projection에 쓰므로 같은 한도 오류가 재발할 수 있다.
정적 데이터 흐름으로 확인했으며 이번 조사에서 별도 동적 재현은 수행하지 않았다.
검증/크기검사는 durable append 이전에 수행하는 회귀검사가 필요하다.

### P2 — 도구 결과의 중간 원문은 저널에도 남지 않음

`ma-runtime/src/tools.rs` execute가 16KiB 전후 head/tail로 잘라 반환하고,
runtime.rs execute_tool_call은 잘린 문자열을 ToolResult로 기록한다.
40KB 파일의 중앙에 둔 고유 증거 문자열이 ToolResult journal에서 사라지는 것을 실제 재현했다.
원본 파일은 남아도 일회성 셸 출력의 중앙값은 복원할 수 없다.

이는 ADR-0002의 후속 출력 제한 선택과 연관된 설계 절충이다. ADR-0001/README의
원문 무손실 보존 표현은 현재 구현 전체에 적용할 수 없다. agent가 출력 파일을 먼저 저장하도록
유도하는 것과 원장에 full output을 별도 보존하는 것의 비용·성능을 비교해야 한다.

### P2 — 압축 검증 실패 시 전략 연속성은 보장되지 않음

`ma-context/src/compaction.rs`는 두 semantic 응답이 무효하거나 감소 목표를 못 맞추면
MechanicallyTrimmed를 반환한다. runtime은 보호 tail 외 session records를 제거하고 기존 brief를
그대로 둔다. 원장은 남지만 모델의 현재 기억에 사실/막다른 길/정확값이 남는다는 보장은 없다.
`brief` note로 완화하지만 `build_request`는 read_effective(note 우선), compact_if_needed는
read(coverage brief)를 사용하므로 실제 주입 기억과 압축 기준도 다르다.

coverage SHA/range 검사는 처리 범위의 구조적 주장 검증이지, 의미가 정확히 보존됐다는
독립 증명은 아니다. 정보보존 실험과 실제 장기 태스크를 연결한 회귀측정이 필요하다.

### P2 — 프롬프트와 실행 계약의 불일치

- prompts/team-tree.md는 restart에서 root만 돌아온다고 설명하지만 실제 resume은 live workers를
  main 직속으로 flatten해 복원한다(ADR-0004 A-light).
- ADR-0001/README에는 flat 2-depth와 main 자동 insight 복사 등의 초기 규칙이 남아 있지만
  실제 코드는 3-depth/neighbor-only를 사용한다. ADR-0004의 상태도 Proposed와 구현완료가 공존한다.
- worker task panic을 감시해 terminal로 확정하는 별도 supervisor가 없다. ADR-0004도 이 한계를
  명시한다. 이번에는 panic 동적 재현을 수행하지 않았다.
- OS 셸은 프로세스 권한으로 실행되며 승인 엔진은 의도적으로 없다. 이를 다중사용자 보안
  sandbox로 평가하지 않았다. workspace 도구의 경로검사와 셸 격리는 다른 보장이다.

## 이번에 수정한 벤치마크 결함

| 문제 | 결과 |
|---|---|
| XBEN-020 내부 의존서비스 오선택 | 세 러너가 공개 진입 앱 ssrf-demo-app을 선택. 내부 expose 변경 없음 |
| 잘못된 TCP/내부 포트·실패한 discovery | 공통 selector/endpoint 경계검증과 실패 분류 보강 |
| 값 없는/범위 밖 실행 옵션 | 동시성1~5, 양의 유한 timeout 검사 및 누락값 거부 |
| suite 패치 실패 무시 | provider 러너가 compose/provider 시작 전에 중단 |
| active network를 orphan으로 오보 | 사용 중/빈 후보/소유권 미확정 구분 |
| 과거 watchdog이 새 attempt 정리 가능 | wrapper token+task lock+정확한 attempt, 살아있는 runner 보호 |
| 취소 후 대기 태스크 계속 시작 | 신규 배정 중단 |
| timeout 정리보다 evidence 확정이 먼저 | bounded cleanup 완료 뒤 확정 |
| shared image ID 강제삭제·전역 prune | 해당 project의 정확한 Repo:Tag만 non-force 정리 |
| setup이 dimage 우회·stale app 참조 | default builder로 owned base→app→runner 의존 빌드 |
| headless 방송 overflow로 실제 flag 유실 | 제출과 이벤트 소비 동시 수행, durable ToolResult 기반 복구, 초기 제출부터 deadline |
| 토큰/turn 로그 스키마 불일치 | usage.jsonl 인식, input+output total 보정 |
| 미측정 도구/대기 지표를0으로 표시 | null로 표시. 무결함·안전성 판정에 사용 금지 |
| 실패 재시도 삭제·최고 성공만 보존 | 모든 attempt 보존, 선택 stamp와 소비 이력 분리 |
| 제외된 solved까지 분자에 포함 | valid-only 분자와 분모 일치 |
| 재시도 소비 누락·cache 이중 합산 | provider 집계에 보존된 완료 attempts 합산, cache 중복 제거 |
| 정상 복구된429도 제외 | 정상종료/timeout은 retry로그만으로 제외하지 않음 |
| Node gate의 obsolete --resume 필수검사 | 현재 fresh --run 계약으로 정정, npm test에 벤치 회귀검사 연결 |

## 벤치마크 수치 해석

조사 중 로컬 보고서에는 Claude 16/19(84.2%), GLM 59/99(59.6%), MiniMax 2/2 scored가 있었다.
이 수치는 서로 같은 모델 런타임·과제수·선택정책의 비교가 아니다. Claude는 호스트 Claude Code,
GLM/MiniMax는 minimal-agent를 사용한다. 따라서 Claude 수치를 이 Rust 팀 에이전트의 점수로
표현하면 안 된다. 미니맥스는 attempted3 중 runtime_fault1이 제외되어 100%로 보였다.
GLM의 37M prompt+893k completion과 total0이라는 자체 모순도 수정 전 보고서에서 확인했다.

현재 생성 파일은 라이브 실행이 갱신하고 있어 감사자가 재생성하지 않았다. 신규 집계 코드는
임시 fixture로 검증했으며, 수정 이후 수치·비용·성공률은 소유자가 완료 경계에서 새로 산출해야 한다.
과거 오선택 XBEN-020, 삭제된 실패 증거, runtime_fault 제외 정책의 영향은 별도로 표시한다.
또한 native의 run-state/harness/audit-manifest 보장은 Claude/MiniMax 러너 전체에 구현된 것은
아니다. Claude 가독성 transcript는 일부 도구 결과를 줄이며 원본 JSONL과 구분해야 한다.
provider별 manifest 부재를 숨기지 않도록 보존 문서의 적용 범위를 명시했다.
기존 duration_s도 빌드 전부터 측정하며 Claude는 teardown까지 포함하므로 solver-only 시간이라는
기존 보고서 문구를 정정했다. 모델 추론속도 비교에는 별도 provider/solver 구간 측정이 필요하다.

## 검증과 범위

- 조사 시작: Docker Rust workspace/all-targets 178개 통과, fmt 및 Clippy 통과.
- 조사 시작: Node launcher3개 통과 뒤 verify-project의 --resume 불일치로 gate 실패. 이번 수정에 포함.
- 코어 진단3개: nested spawn 실패/slot잔류, tool 중앙증거 소실, delayed event 수신증거 소실 재현.
  재현 소스는 audit-probes-2026-09-05.rs에 보존했다. 기대결함을 확인하는 진단용이며 정상계약
  acceptance test가 아니다. tests/에 임시로 두고 dbuild로 실행 후 제거했다.
- 최종 통합 검증 결과는 함께 제공하는 verification-2026-09-05.md를 참조한다.
- 신규 실제 XBOW 실행, provider 호출, browser/interactive TUI smoke, release image 실빌드는
  수행하지 않았다. 라이브 작업의 리소스·상태를 바꾸지 않기 위한 범위 제한이다.

우선순위는 실제 depth2 runtime 실행/정리 회귀 → note append 전 검증 → 문맥·원문 보존 계약
정합화 → 동일 하니스/과제/예산의 재측정이다. 벤치마크 운영자는 RUNBOOK/HANDOFF의 분류 기준을
따르고, 코어 전략/메모리 문제를 타깃 인프라 문제로 보고하지 않아야 한다.

관련 변경 설명: [벤치마크 신뢰성 개선 보고서 — 원인·변경 전후·검증](benchmark-improvements-2026-09-05.md)
