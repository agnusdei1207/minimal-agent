# 용어집 (GLOSSARY)

인텐트·코드·테스트·UI가 공유하는 한 언어. 개념마다 여기 한 단어를 쓰고, 코드 식별자는
괄호로 병기한다. 새 개념은 코드에 등장하기 전에 여기 먼저 등록한다.

- **Agent(에이전트, `AgentId`)** — 팀 트리의 한 노드. 자기 전장 노트를 갖고 도구를 실행하며 한 턴씩 돈다.
- **Main(메인, `AgentId::main`)** — 팀 리더(depth 0). 사람의 최신 지시가 즉시 목표. 최종 답을 소유한다.
- **Worker(워커)** — main이 아닌 에이전트(depth 1~2). depth 1은 자식을 스폰할 수 있고, depth 2는 leaf다.
- **Node role(노드 역할)** — 자식이 있으면 internal-node(분해·팬아웃·충실 집계), 없으면 leaf(직접 실행·핵심 보고).
- **Depth(깊이, `AgentDepth`, `MAX_DEPTH=2`)** — 트리 깊이. 0=main, 1=child, 2=grandchild(leaf). 더 깊게 못 만든다.
- **Team tree(팀 트리)** — 경계 지어진 3단계 계층. 노드당 활성 ≤10 (`MAX_TEAM_SIZE`).
- **Neighbor(이웃)** — 직계 부모·직계 자식·같은 부모의 형제. 통신은 이웃끼리만 허용된다.
- **Faithful upward reporting(충실한 상향 보고)** — 각 홉은 검증된 핵심(flag·값·PoC·증거)을 요약해 없애지 않고
  그대로 부모에게 올린다. leaf는 성공이든 dead end든 반드시 핵심을 보고한다(침묵 금지).
- **Journal(저널, `RunJournal`, `JournalEvent`)** — append-only 사건 원장. 세그먼트·블롭·체크섬·리플레이.
  압축이 컨텍스트에서 무엇을 걷어내도 원장은 무손실(INTENT-0001 §9).
- **Brief / Battlefield note(전장 노트, `AgentBriefStore`)** — 에이전트별 압축·범주화된 메모리. 컨텍스트 압축을
  견디고 매 턴 최상단에서 재읽힌다. `brief` 도구로 스스로 유지한다.
- **Compaction(압축, `compaction.rs`)** — 컨텍스트가 임계(80%)를 넘으면 의미를 보존하며 brief로 요약한다.
  공급된 소스 구간을 버리지 않음을 커버리지(`covered_ranges`)로 증명한다. 실패 시 기계적 폴백으로 멈추지 않는다.
- **Coverage(커버리지, `CompactionCoverage`)** — 압축이 어느 시퀀스 구간·insight를 요약으로 대체했는지의 증명.
- **Engagement(교전, `Engagement`)** — 인가된 보안 작업·CTF의 목표·범위·flag 규율. 저널에 `EngagementSet`로 영속되고
  재개 시 복구된다(INTENT-0002).
- **Flag(플래그)** — 교전의 성공 증거 토큰. 정규식으로 추출된다.
- **Finding / Final(발견·최종, `report` 도구)** — Finding은 임의 에이전트가 남기는 발견, Final은 main만 남기는 최종 답.
- **Tool(도구, `BuiltinTools`)** — 에이전트가 호출하는 행위. `bash`·`tmux`·`workspace`·`team`·`journal`·`report`·`brief`. 큰 출력은 절단된다.
- **Bash tool(`bash` 도구)** — 셸 명령 실행 도구. 프로세스는 `bash -lc`. "shell"은 도메인 용어로만 남긴다.
- **Provider(프로바이더, `OpenAiChatProvider`)** — OpenAI 호환 LLM 클라이언트. 스트리밍·재시도·델타 조립.
- **Turn / Step(턴·스텝)** — 한 번의 모델 호출과 그에 따른 도구 실행. 도구 호출 없이 final도 없으면 턴 한도에서 멈춘다.
- **Insight(인사이트)** — 에이전트 간 메시지 종류 중 하나(progress·insight·request·final).
- **Dead end(막다른 길)** — 3~5회 실패로 차등 신호가 없는 공격 범주. brief에 표시하고 다시 돌아가지 않는다.
