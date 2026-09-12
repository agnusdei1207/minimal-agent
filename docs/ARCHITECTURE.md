# 구조 지도 (ARCHITECTURE)

지금 코드가 어떤 모듈로 이뤄져 있고 각각 왜 있는가. 한 화면 지도이며 코드를 복제하지 않는다.
코드 구조가 바뀌면 같은 커밋에서 이 표를 갱신한다. "근거"는 이 저장소의 결정기록 관례인 `docs/intents/`를 가리킨다.

단일 crate `pentesting`: 라이브러리 `src/lib.rs`(공개 모듈 12개 재노출) + 바이너리 `src/main.rs`
(바이너리 전용 `cli`·`headless` 모듈을 선언하고 `cli::main()`으로 위임). 과거의 `crates/ma-*`
워크스페이스 분할과 평면 파일 구조는 도메인 단위 디렉터리 모듈로 통합·재편됨.

| 모듈 | 책임 (한 줄) | 공개 인터페이스 요지 | 근거 |
|---|---|---|---|
| `domain/` | 코어 값 타입·불변식·검증 | `AgentId`·`AgentState`·`AgentDepth`(0..=2)·`ContextBudget`, 토큰 추정, 입력 경계 | INTENT-0001, INTENT-0004 |
| `coordinator/` | 팀 레지스트리·스폰·이웃 라우팅·캐스케이드 | `Coordinator`(create_worker·send·recall·`active_team_size`·descendant 캐스케이드) | INTENT-0001, INTENT-0004 |
| `journal/` | append-only 사건 원장(무손실) | `RunJournal`·`JournalEvent`·`ReplayedEvent`, 세그먼트·블롭·체크섬·리플레이 | INTENT-0001 §9 |
| `brief/` | 에이전트별 전장 노트 저장·투영 | `AgentBriefStore`(initialize·read·note 기록·main 런타임 블록 동기화) | INTENT-0001 §8 |
| `compaction/` | 의미 보존 컨텍스트 압축→brief | `compact()`·`CompactionOutcome`(커버리지 증명 + 기계적 폴백) | INTENT-0001 §9, INTENT-0002 §3.14 |
| `engagement/` | 인가 교전·CTF 규율 값·flag 추출 | `Engagement`(목표·범위·flag 정규식), 저널 영속·재개 복구 | INTENT-0002 |
| `provider/` | OpenAI 호환 LLM 클라이언트 | `OpenAiChatProvider`·`OpenAiConfig`, 스트리밍·재시도·델타 조립 | INTENT-0001 |
| `settings/` | 프로바이더 설정 저장·env 파싱 | `ProviderSettingsStore`·`parse_token_input`·`build_provider` | INTENT-0002 |
| `tools/` | 도구 레지스트리·실행·출력 절단 | `BuiltinTools`(`definitions`·`execute`: `bash`·`tmux`·`workspace`·`team`·`journal`·`report`·`brief`), `team finish`→부모 버블업 | INTENT-0001, INTENT-0003, INTENT-0004 |
| `prompt/` | 시스템 프롬프트 조립 | `builder.rs`의 `build_system`(POSITION별 블록 선택), `constants.rs`에 코어 프롬프트 11종 `include_str!` + 교전 교리 3종 accessor fn | INTENT-0002 |
| `runtime/` | 턴 루프·오케스트레이션 | `TeamRuntime`(create·resume·submit_user·spawn_worker), worker 턴 루프(`worker.rs`가 `prompt::build_system`으로 위임), `render_position` POSITION 블록 주입, 이벤트 스트림 | INTENT-0001, INTENT-0002, INTENT-0004 |
| `headless/` | TUI 없는 실행 엔진(바이너리 전용) | `runner`(런 드라이브)·`observation`(`RuntimeEvent` 투영·저널 증거·디버그 로그)·`constants` | INTENT-0002 |
| `cli/` | CLI 인자·실행 배선(바이너리 전용) | `args`(`Cli`·`Command`·`RunArgs`·`build_engagement`)·`runner`(프로바이더 조립·세션 배너)·`plain`·`inspect`, env(`PENTESTING_*`) | INTENT-0002, INTENT-0006 |
| `tui/` | 터미널 UI(상태·전사 렌더) | `mod`(상태·`/status` 모달)·`view`(상태줄)·`markdown`·`command`·`model_setup`·`theme` | INTENT-0001, INTENT-0002 |
| `main.rs` | 바이너리 진입점 | `mod cli`·`mod headless` 선언 후 `cli::main()` 위임 | INTENT-0006 |
| `lib.rs` | crate 루트·모듈 재노출 | 위 라이브러리 모듈 12개 공개 | INTENT-0001 |

## 프롬프트 (코어에 컴파일됨)

`prompts/*.md`는 `include_str!`로 바이너리에 구워지며 `src/prompt/builder.rs`의 `build_system`이 위치(POSITION)에 따라
조립한다: 항상 `execution-mandate`(반사적 거부 방지, 보호됨)·`authorized-engagement`·`team-tree`, main은
`main-role`·`fan-out`·`self-management`, 비-main은 자식 유무로 `node-internal` 또는 `node-leaf`.
교전 계열 3종(`authorized-engagement`·`operating-style`·`ctf-solve-loop`)도 `src/prompt/constants.rs`가
소유하며 교전 종류에 따라 덧붙는다. 프롬프트가 정본이며 바뀌면 재빌드(Docker)해야 반영된다.
