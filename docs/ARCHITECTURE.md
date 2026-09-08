# 구조 지도 (ARCHITECTURE)

지금 코드가 어떤 모듈로 이뤄져 있고 각각 왜 있는가. 한 화면 지도이며 코드를 복제하지 않는다.
코드 구조가 바뀌면 같은 커밋에서 이 표를 갱신한다. "근거"는 이 저장소의 결정기록 관례인 `docs/adr/`를 가리킨다.

단일 crate `pentesting`(라이브러리 `src/lib.rs` + 바이너리 `src/main.rs`). 과거의 `crates/ma-*`
워크스페이스 분할은 되돌려 단일 패키지로 통합됨.

| 모듈 | 책임 (한 줄) | 공개 인터페이스 요지 | 근거 |
|---|---|---|---|
| `domain.rs` | 코어 값 타입·불변식·검증 | `AgentId`·`AgentState`·`AgentDepth`(0..=2)·`ContextBudget`, 토큰 추정, 입력 경계 | ADR-0001, ADR-0004 |
| `coordinator.rs` | 팀 레지스트리·스폰·이웃 라우팅·캐스케이드 | `Coordinator`(create_worker·send·recall·`active_team_size`·descendant 캐스케이드) | ADR-0001, ADR-0004 |
| `journal.rs` | append-only 사건 원장(무손실) | `RunJournal`·`JournalEvent`·`ReplayedEvent`, 세그먼트·블롭·체크섬·리플레이 | ADR-0001 §9 |
| `brief.rs` | 에이전트별 전장 노트 저장·투영 | `AgentBriefStore`(initialize·read·note 기록·main 런타임 블록 동기화) | ADR-0001 §8 |
| `compaction.rs` | 의미 보존 컨텍스트 압축→brief | `compact()`·`CompactionOutcome`(커버리지 증명 + 기계적 폴백) | ADR-0001 §9, ADR-0002 §3.14 |
| `engagement.rs` | 인가 교전·CTF 규율 값·flag 추출 | `Engagement`(목표·범위·flag 정규식), 저널 영속·재개 복구 | ADR-0002 |
| `provider.rs` | OpenAI 호환 LLM 클라이언트 | `OpenAiChatProvider`·`OpenAiConfig`, 스트리밍·재시도·델타 조립 | ADR-0001 |
| `settings.rs` | 프로바이더 설정 저장·env 파싱 | `ProviderSettingsStore`·`parse_token_input`·`build_provider` | ADR-0002 |
| `tools.rs` | 도구 레지스트리·실행·출력 절단 | `BuiltinTools`(`definitions`·`execute`: `bash`·`tmux`·`workspace`·`team`·`journal`·`report`·`brief`), `team finish`→부모 버블업 | ADR-0001, ADR-0003, ADR-0004 |
| `runtime.rs` | 턴 루프·오케스트레이션·시스템 프롬프트 조립 | `TeamRuntime`(create·resume·submit_user·spawn_worker), `render_position` POSITION 블록 주입, 이벤트 스트림 | ADR-0001, ADR-0002, ADR-0004 |
| `tui/` | 터미널 UI(상태·전사 렌더) | `mod`(상태·`/status` 모달)·`view`(상태줄)·`markdown`·`command`·`model_setup`·`theme` | ADR-0001, ADR-0002 |
| `main.rs` | CLI 진입·인자 파싱·실행 배선 | 인자·env(`PENTESTING_*`, 레거시 `MINIMAL_AGENT_*` 폴백)·상태 디렉토리 | ADR-0002, ADR-0006 |
| `lib.rs` | crate 루트·모듈 재노출 | 위 모듈 공개 | ADR-0001 |

## 프롬프트 (코어에 컴파일됨)

`prompts/*.md`는 `include_str!`로 바이너리에 구워지며 `runtime.rs`의 `build_system`이 위치(POSITION)에 따라
조립한다: 항상 `execution-mandate`(반사적 거부 방지, 보호됨)·`authorized-engagement`·`team-tree`, main은
`main-role`·`fan-out`·`self-management`, 비-main은 자식 유무로 `node-internal` 또는 `node-leaf`. 프롬프트가 정본이며
바뀌면 재빌드(Docker)해야 반영된다.
