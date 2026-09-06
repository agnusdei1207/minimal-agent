# ADR-0005: Web Browser Automation Discipline, CLI Architecture, and On-Demand Traffic Interception

- Status: Accepted (반영 완료 · 스킬 프롬프트 주도 아키텍처)
- Created: 2026-09-06 +09:00
- Version target: 0.111.0
- Repository: `agnusdei1207/minimal-agent`
- Relation: ADR-0001 (Clean-Room Core), ADR-0002 (Authorized Engagement), ADR-0003 (Shell Primitives), ADR-0004 (Bounded Three-Depth Hierarchy).
- 상태 주석:
  - **인프라 및 안전 가드 계층 (Docker `--init` 좀비 수거, 원시 크롬 오용 차단 가드 스크립트):** 반영 완료 (Implemented).
  - **접근성 트리(Accessibility Tree) 및 핀포인트 DOM 관측 파이프라인:** 반영 완료 (Implemented via Skill Prompts). 별도의 Rust 런타임 코드나 복잡한 함수 툴 추가 없이, 시스템 프롬프트(`prompts/skills/02-web-application.md`)를 통해 관측 및 행동 규약을 확립.

---

## 1. 한 문장 결정 (One-Sentence Decision)

`minimal-agent`는 웹 브라우저 제어와 네트워크 트래픽 가로채기를 위해 Rust 코어에 무거운 브라우저 드라이버나 자체 프록시 엔진을 구축하지 않고,
(1) 브라우저 자동화는 CDP 기반의 경량 CLI 도구(`agent-browser`)와 원시 크롬 오용 방지 가드를 통해 표준 `bash` 도구로 구동하며,
(2) 자바스크립트 다이얼로그(`alert()`, `confirm()`, `prompt()`)의 자동 수락 및 **스킬 프롬프트 주도형 컴팩트 접근성 트리(Accessibility Tree, ~200토큰) 스냅샷 관측 파이프라인을 배선**하고,
(3) 트래픽 가로채기는 상시 강제 라우팅(타깃 미도달 시 프록시 502 에러 페이지로 인한 환각 유발)을 배제하고 컨테이너 내 기성 도구(`mitmdump`/`caido-cli`)와 `http_proxy` 환경변수를 통한 **필요 시(On-Demand) 활성화**로 제한하며,
(4) 컨테이너 런타임에 Docker `--init`(tini PID 1)을 강제하여 브라우저 및 서브프로세스의 좀비(`<defunct>`) 잔존과 그로 인한 에이전트 대기 환각 루프를 원천 차단한다.

---

## 2. 배경 및 실전 실패 사례 (Design Dilemma & Case Studies)

### 2.1 XBEN-013-24 사건: 모달 다이얼로그 락업과 좀비 루프
XBOW-104 벤치마크의 Dalgona Reflected XSS 과제(`XBEN-013-24`)에서 autonomous loop의 치명적인 결함이 관측되었다:
1. **서버 사이드 채점기 정상 동작:** 타깃 서버의 PhantomJS 2.1.1 채점 봇은 키워드 분할 페이로드(`<svg onload=window['al'+'ert']('XSS')>`)가 포함된 단 1회의 HTTP 요청에 대해 **0.03초 만에 플래그를 정상 응답**했다.
2. **에이전트의 경로 이탈 (Catastrophic Divergence):** 서버 응답 본문을 확인하는 대신, 최초 문법 에러(`TypeError: alert is not a function`)를 본 모델(GLM-5.3-Flash)이 공황 상태에 빠져 컨테이너 로컬 `/tmp`에서 원시 브라우저 덤프(`google-chrome --headless=old --dump-dom ...`)를 실행했다.
3. **모달 동기 락업 (Modal Freeze Trap):** Chromium은 `alert()` 호출 시 사용자가 확인 버튼을 누를 때까지 자바스크립트 실행과 DOM 렌더링을 완전히 정지(freeze)시킨다. 헤드리스 환경에서 다이얼로그 핸들러가 없자 브라우저 프로세스가 영구 정지 상태에 빠졌다.
4. **좀비 프로세스 착시 (<defunct> Zombie Loop):** 자식 쉘 스크립트가 타임아웃으로 강제 종료된 후에도 고아 크롬 서브프로세스가 회수되지 않고 `ps aux`에 `<defunct>`로 남았다. 에이전트는 이를 보고 "크롬이 아직 렌더링 중"이라고 오판하여 **130턴(15분 이상) 동안 `sleep 60`을 반복하며 전체 시간 예산을 낭비**했다.

### 2.2 XBEN-008-24 사건: 호스트 메모리 압박과 SIGKILL
컨테이너 내부에서 무거운 원시 크롬 인스턴스가 기동되면서 300MB~700MB의 RAM을 순간 점유했고, 동시 실행 환경에서 호스트 메모리 한계에 도달하여 컨테이너가 OS 커널 OOM-killer에 의해 `SIGKILL`되는 사태가 발생했다.

### 2.3 채점관 브라우저 vs 공격자 브라우저의 이원성 혼동 (The Dual-Browser Dichotomy)
모의해킹/CTF 과제에서 브라우저는 두 가지 완전히 다른 목적으로 쓰인다:
- **서버 사이드 채점관 (PhantomJS):** 타깃 컨테이너 내부에서 동작하며 XSS 트리거 여부를 판정해 플래그를 지급하는 **필수 불가결한 판정관**.
- **공격자 사이드 브라우저 (Chromium):** 공격 에이전트가 DOM을 보거나 렌더링 결과를 확인할 때 쓰는 **보조 도구**. 대부분의 취약점은 `curl`과 직접 HTTP 통신만으로 100% 해결 가능하므로 공격자 측의 무거운 로컬 브라우저 기동은 대다수 상황에서 불필요한 위험 요소다.

---

## 3. 결정 내용 및 이행 현황 (Decisions & Status)

| 계층 | 내용 | 상태 |
| :--- | :--- | :--- |
| **인프라** | Docker `--init` 강제 적용 (PID 1 tini의 좀비 프로세스 0ms 즉시 회수) | **[반영 완료]** |
| **안전 가드** | PATH 상의 원시 `google-chrome`/`chromium` 차단 가드 스크립트 배선 | **[반영 완료]** |
| **코어 철학** | Rust 코어 내 브라우저/프록시 엔진 자체 개발 전면 기각 (Zero-Bloat) | **[반영 완료]** |
| **접근성 트리 & DOM 관측** | 스킬 프롬프트 기반 접근성 트리(`snapshot -i -c`) 및 핀포인트 DOM 추출 파이프라인 | **[반영 완료 · 프롬프트 주도]** |
| **트래픽 프록시**| `mitmproxy` 온디맨드 쉘 연동 레시피 가이드 (상시 강제 라우팅 기각) | **[반영 완료]** |

---

### 3.1 Rust 코어 비대화 금지 불변식 (Rust Core Zero-Bloat) [반영 완료]
- Rust 코어(`crates/ma-runtime`)에 브라우저 제어 엔진(CDP 라이브러리, Playwright 바인딩 등)이나 HTTP/HTTPS MITM 프록시 엔진을 새로 작성하지 않는다.
- ADR-0001의 최소 코어 철학과 ADR-0003의 쉘 실행 모델에 따라, 모든 외부 도구 조작은 표준 `bash` 및 `tmux` 도구를 통해 CLI 수준에서 완결한다. Rust 코드 수정은 **0줄**을 유지한다.

### 3.2 원시 브라우저 바이너리 트랩 차단 (Trapping Raw Binaries) [반영 완료]
- 에이전트가 `which google-chrome`을 감지하고 무지성으로 `--dump-dom` 스크립트를 작성하는 함정을 차단하기 위해, 컨테이너 PATH(`/usr/local/bin`)의 원시 `google-chrome`, `chromium` 심볼릭 링크를 제거하고 안전한 가드 래퍼로 대체했다.
- 가드 래퍼는 실행 시 즉각 에러를 출력하고 에이전트에게 직접 HTTP 타격(`curl`, Python `requests`) 또는 표준 `agent-browser`를 쓰도록 안내하여 즉각적 피드백을 제공한다.

### 3.3 스킬 프롬프트 주도형 접근성 트리(Accessibility Tree) 및 핀포인트 DOM 관측 파이프라인 [반영 완료]

날것의 HTML 덤프(`document.body.outerHTML`, 무차별 `curl`)는 두 가지 치명적 문제를 유발한다:
1. **16KB 중간 잘림(Middle-Truncation) 재앙:** `ma-runtime`의 `truncate_tool_content`는 16KB 초과 시 앞뒤만 남기고 본문 가운데를 `[... bytes omitted ...]`로 잘라버린다. 웹 타깃의 핵심인 `<form action="...">`, `<input name="...">`, CSRF hidden 토큰이 본문에 위치하므로 에이전트가 이를 보지 못해 맹목적 추측 루프에 빠진다.
2. **토큰 낭비 및 문맥 오염:** 수만 줄의 Tailwind/Bootstrap CSS 클래스, 인라인 SVG, 번들 JS 코드가 수만 토큰을 소모한다.

**해결 원칙: 제로-런타임-코드(Zero Runtime Code) & 프롬프트 주도(Prompt-Driven)**
Rust 코어에 무거운 브라우저 바인딩이나 별도 함수 도구를 추가하지 않고, 컨테이너 내 기설치된 `agent-browser` CLI와 `prompts/skills/02-web-application.md` 스킬 프롬프트를 통해 에이전트의 관측/행동 규약을 통제한다:

```
[표준 웹 브라우징 & 관측 루프]
1. 탐색 시작: agent-browser open <target-url>
2. 컴팩트 관측: agent-browser snapshot -i -c  ──▶ [Role + Name + State + @eN 식별자, ~150-300 토큰 초경량 트리]
3. 요소 조작: agent-browser click @e3 / fill @e2 "payload" / press Enter
4. 갱신 감지: 페이지 이동 또는 DOM 변경 시 agent-browser snapshot -i -c 재호출 (Stale Ref 방지)
5. 핀포인트 검증: agent-browser get html @e2  ──▶ [XSS 페이로드 반영 등 필요 노드의 innerHTML만 수십 바이트 확인]
```

- **접근성 트리 기본화:** `snapshot -i` (상호작용 가능 요소 한정) 및 `-c` (컴팩트 모드)를 기본 관측 규약으로 정립하여 HTML 태그를 배제하고 스크린 리더용 Role, Name, State 정보만을 `@e1`, `@e2` 형태로 정제.
- **참조 식별자(`@eN`) 기반 제어:** 복잡한 CSS 셀렉터 대신 `@eN` 참조를 활용하여 클릭, 텍스트 입력을 완결.
- **핀포인트 DOM 조회:** XSS 스크립트 반영 여부나 세부 태그 속성 확인이 필요할 때만 `agent-browser get html @eN` 또는 `get attr @eN <attr>`으로 해당 요소만 조회.
- **텍스트/마크다운 덤프 연계 (`w3m` / `html2text`):** 블로그, CMS, 문서성 웹 사이트 탐색 시 날것의 HTML 대신 `w3m -dump -cols 120 <url>` 또는 `curl -s <url> | python3 -m html2text`로 순수 텍스트 레이아웃만 추출하여 토큰을 80~90% 절감.
- **정적 HTTP/크롤링 위생 (BeautifulSoup):** `curl`을 쓸 때도 HTML 전체를 표준 출력에 쏟아내지 않고, BeautifulSoup 원라이너(`python3 -c "import bs4..."`)나 정밀 `grep`을 파이프로 연결해 `<form>`, `<input>`, 주석(`<!-- -->`)만 좁혀서 관측하도록 스킬 프롬프트에 명시.

### 3.4 온디맨드 트래픽 가로채기 및 원시 네트워크 탭 (On-Demand Proxy & Raw Network Tap) [반영 완료]
- 기존의 상시 강제 프록시(`ALL_PROXY`) 구조는 타깃 서비스 미기동 또는 일시 장애 시 프록시 데몬 자체의 502 Bad Gateway HTML 응답을 타깃 서버의 응답으로 에이전트가 오인하는 치명적 환각 부작용을 유발한다.
- 따라서 `minimal-agent`는 **상시 강제 라우팅을 명시적으로 기각**하고 **온디맨드(On-Demand) 방식**을 채택한다:
  - 컨테이너에 기설치된 `mitmproxy`(`mitmdump`) 또는 경량 로깅 프록시(`scripts/logging-proxy.py`)는 필요 시에만 기동하며, 에이전트가 패킷 분석이 필요한 특정 단계에서만 `export http_proxy=http://127.0.0.1:8080`을 일시 주입하여 활용한다.
  - 컨테이너에 `NET_RAW`, `NET_ADMIN` 권한(`docker/compose.yaml`)을 부여하여 별도의 무거운 프록시 프레임워크 없이도 `tcpdump`, `scapy`, `tshark`를 통해 원시 네트워크 탭(Network Tap) 및 양방향 패킷 조작을 OS 네이티브 수준에서 즉시 수행한다.

### 3.5 계층적 멀티 에이전트 컨텍스트 격리 (Hierarchical Multi-Agent Context Isolation, ADR-0004) [반영 완료]
- 대규모 웹 탐색 및 소스코드 분석 시 단일 에이전트가 모든 터미널 출력(수십 KB)을 자신의 문맥에 누적하면 급격한 컨텍스트 폭발과 추론 품질 저하가 발생한다.
- 이를 해결하기 위해 ADR-0004의 **3-Depth 계층 오케스트레이션**을 웹 정찰 파이프라인에 직결한다:
  1. **Main (Planner / Coordinator):** 직접 웹 브라우징이나 덤프를 수행하지 않고, 전체 공격 전략 수립, 워커 생성(`team create --role "Recon-Web"`), 최종 결과 취합만 담당.
  2. **Worker (Leaf Node):** `agent-browser`, `w3m`, `curl` 등 무거운 터미널 정찰 작업을 전담하여 실행. **워커의 대용량 웹 터미널 출력은 Main의 문맥을 절대 오염시키지 않음**.
  3. **정형 메시지 전달:** 워커는 탐색 완료 후 4대 정형 메시지 중 `Insight` 또는 `Progress`를 통해 정제된 핵심 단서와 원시 값(Exact Values, 예: `/login 엔드포인트 파라미터 user, pass`)만을 상위로 전달하여 문맥 크기를 완벽히 보존.

### 3.6 컨테이너 좀비 프로세스 자동 회수 (`--init` 강제) [반영 완료]
- `benchmarks/harness/runner.mjs` 및 모든 에이전트 컨테이너 기동 명령에 Docker `--init` 플래그를 강제했다.
- Docker 내장 경량 init(`tini`)이 PID 1을 맡아, 충돌하거나 비정상 종료된 브라우저/서브프로세스의 고아 프로세스를 0ms 내에 완전히 회수(reap)하여 `ps aux` 상의 `<defunct>` 좀비 착시 루프를 영구 제거했다.

### 3.7 모델 사전학습 공백 극복을 위한 이중 주입 구조 (Dual Injection Architecture) [반영 완료]
- **사전학습 가중치 공백 (Pre-training Gap):** `agent-browser`는 최근(2025/2026) 등장한 CLI 도구이므로, GLM-5.3, DeepSeek-v4, Llama 3 등 일반 LLM의 사전학습 가중치에는 해당 명령어 명칭이나 옵션 체계(`snapshot -i -c`, `@e1`, `click`, `fill`)가 학습되어 있지 않다.
- **지연 로딩(Lazy Loading)의 한계와 해법:** 온디스크 스킬 파일(`/opt/minimal-agent/skills/02-web-application.md`)에만 사용법을 둘 경우, 에이전트가 자발적으로 해당 카드를 읽지 않는 한 도구의 존재 자체를 인지하지 못하고 과거 익숙한 `selenium`/`playwright`/`google-chrome --dump-dom` 스크립트를 작성하여 모달 프리징 함정에 빠지게 된다.
- **상시 주입(시스템 프롬프트) + 지연 로딩(스킬 카드) 이원화:**
  1. **상시 앵커링 (Core Anchor, ~40토큰):** 모든 에이전트에게 매 턴 기본 주입되는 `prompts/tradecraft.md`의 `BROWSER AUTOMATION` 섹션에 `agent-browser`의 존재, 기본 구문(`open`, `snapshot -i -c`, `click`, `fill`), 세션 격리(`--session`), 스킬 카드 참조 경로를 3줄로 명시하여 도구 발견성을 100% 보장.
  2. **심층 프로토콜 (Deep Methodology):** 세부적인 SPA 로드 대기, DOM 정밀 검증, 다계층 텍스트 파이프라인(`w3m`/`html2text`)은 `/opt/minimal-agent/skills/02-web-application.md`에 배치하여 컨텍스트 낭비를 최소화.

### 3.8 멀티 에이전트 팀 브라우저 세션 격리 (`--session <agent-id>`) [반영 완료]
- `minimal-agent`는 단일 컨테이너 내부에서 최대 10명의 에이전트(Main, Workers)가 병렬로 작동한다(ADR-0004).
- 별도의 세션 플래그 없이 `agent-browser open`을 호출하면 모든 에이전트가 기본 Chromium 인스턴스를 공유하게 되어, 한 에이전트가 다른 에이전트의 페이지를 덮어쓰고 DOM `@eN` 참조를 무효화하는 치명적인 상호 간섭(Race Condition)이 발생한다.
- 따라서 모든 브라우저 호출 규약에 `--session <agent-id>` (예: `agent-browser --session "$AGENT_ID" open <url>`)를 의무화하여 독립된 브라우징 컨텍스트를 완벽히 격리했다.

### 3.9 SPA 비동기 렌더링 안정화 및 세션 수명 관리 [반영 완료]
- **비동기 렌더링 대기:** React, Vue 등 단일 페이지 애플리케이션(SPA)에서 페이지 이동 직후 DOM이 완성되기 전에 스냅샷을 찍어 빈 껍데기 화면을 관측하는 현상을 방지하기 위해 `agent-browser wait --load networkidle` 규약을 추가했다.
- **유휴 세션 수명(3분):** 컨테이너 데몬의 `IDLE_TIMEOUT=180000`(3분 유휴 자동 종료) 메커니즘을 명시하여, 장시간 오프라인 작업 후 브라우저 세션이 초기화되는 함정에 빠지지 않도록 가이드를 수립했다.

---

## 4. 제거된 군더더기 및 안티패턴 (Eliminated Redundancies & Anti-Patterns)

본 결정을 통해 시스템에서 굳이 유지할 필요가 없었던 다음 요소들을 공식적으로 정리·배제했다:

1. **`benchmarks/harness/Dockerfile.runner`의 중복 패키지 설치 레이어:**
   - 기존 러너 이미지에서 `apt-get update && apt-get install -y netcat-openbsd iputils-ping`을 수행했으나, 해당 패키지들은 이미 `docker/runtime-base.Dockerfile`에 모두 포함되어 있었음. 중복 빌드 레이어를 제거하여 빌드 시간과 디스크 사용량을 절감.
2. **PATH 상의 원시 크롬 실행 링크 노출:**
   - 원시 브라우저를 PATH에 무방비로 노출하여 에이전트가 로컬 브라우저 자동화 스크립트를 작성하게 유도하던 환경적 결함을 제거.
3. **Rust 코어 내 브라우저/프록시 드라이버 작성 시도:**
   - 코어의 역할을 "Linux OS 및 CLI 환경을 오케스트레이션하는 초경량 런타임"으로 한정하고 네트워크/브라우저 엔진 개발 시도를 사전에 기각.

---

## 5. 검토 및 기각된 대안 (Considered & Rejected Alternatives)

본 결정에 앞서 다각도로 검토되었으나, 명확한 부작용과 설계 철학 위배로 인해 공식 기각된 대안들의 상세 사유를 영구 보존한다:

| 기각된 대안 | 기각 사유 요약 | 최종 채택된 대체 해법 |
| :--- | :--- | :--- |
| **1. Rust 코어에 브라우저/CDP 엔진 및 HTML 파서 직접 구현** | 수천 줄의 비동기 네트워크 코드가 유입되어 ADR-0001의 Lean Core 원칙 파괴. 코드는 영구 부채이며 프롬프트/스킬로 해결 가능한 문제를 코드로 구현하는 것은 자원 낭비. | 컨테이너 내 기성 `agent-browser` CLI + 스킬 프롬프트 기반 접근성 트리 관측 |
| **2. 날것의 원시 HTML 직접 읽기 (`curl`, `outerHTML`)** | 80~90%가 CSS/JS/SVG 쓰레기 코드로 토큰 폭망. 16KB 문맥 상한 초과 시 `truncate_tool_content`가 본문 중간의 Form/Input/토큰을 잘라내 맹목적 추측 루프 유발. | `agent-browser snapshot -i -c` (접근성 트리, ~200토큰) 및 필요 노드만 핀포인트 조회 (`get html @eN`) |
| **3. 별도 브라우저 함수 도구(`browser_click`, `browser_type` 등) 신설** | 5~10개 도구 스키마 정의만으로 매 턴 수천 토큰 고정 잠식(3-Layer Golden Rule 위배). Bash와의 파이프라인 결합 불가능. | 기존 `bash` 도구 하나로 `agent-browser` 명령어를 실행하는 단일 인터페이스 유지 |
| **4. 전역 상시 강제 프록시 (Always-on `ALL_PROXY` 방식)** | 타깃 미도달/재시작 시 프록시 데몬(Caido/mitmproxy) 자체의 502/500 에러 페이지를 타깃 응답으로 에이전트가 환각하여 잘못된 경로로 이탈. | 패킷 가로채기가 꼭 필요한 단계에서만 `export http_proxy`를 설정하는 온디맨드(On-Demand) 방식 |
| **5. 모든 도구의 전면 백그라운드/비동기 호출 모델** | 인과성 파괴, 결과 확인을 위한 무의미한 폴링(`sleep`) 루프로 턴 수 3~4배 폭증, 백그라운드 프로세스 누적으로 인한 OOM 크래시. | "95% 동기식(`bash`) + 5% 명시적 비동기(`tmux`)" 이원화 모델 (ADR-0003 §3.5) |
| **6. 쉘 전역 타임아웃 일괄 단축 (예: 30초 고정 제한)** | `nmap`, `ffuf`, `dirb` 등 정상적인 고지연 대용량 정찰 도구들의 정상 실행을 방해하여 분석 자체가 불가능해짐. | `bash` 도구에 `timeout_secs` 선택 파라미터 자율권을 부여하여 에이전트가 소요 시간을 직접 판단 (ADR-0003 §3.4) |
| **7. 로컬 무거운 자동화 프레임워크 스크립트 작성 유도 (`playwright`, `puppeteer`, 원시 `google-chrome`)** | `alert()` 다이얼로그 모달 락업으로 인한 15분 타임아웃 프리징 및 `<defunct>` 좀비 착시 루프. 인스턴스당 300~700MB RAM 점유로 호스트 OOM 유발. | PATH 상의 원시 크롬 실행을 가드 래퍼로 차단하고, 다이얼로그 자동 수락 CLI(`agent-browser`) 강제 |
| **8. 스킬 파일에만 브라우저 도구를 두고 시스템 프롬프트 안내를 생략하는 방식 (순수 지연 로딩 의존)** | 일반 LLM(GLM, DeepSeek 등)의 사전학습 가중치에 최신 도구인 `agent-browser`가 없어, 스킬을 열어보지 않으면 도구 존재를 인지하지 못하고 과거의 Selenium/크롬 작성 함정으로 회귀. | 시스템 프롬프트(`tradecraft.md`)에 ~40토큰 상시 앵커링 주입 + 스킬 카드에 심층 프로토콜을 배치하는 이중 주입 구조(Dual Injection) 채택 (§3.7) |
| **9. 멀티에이전트 브라우저 단일 세션 공유 (세션 격리 플래그 생략)** | 최대 10명의 에이전트(Main, Workers)가 병렬로 웹 정찰 시 단일 브라우저 인스턴스를 공유하여 페이지 덮어쓰기 및 `@eN` 참조 파괴(Race Condition) 발생. | 모든 브라우저 호출 규약에 `--session <agent-id>`를 의무화하여 컨텍스트 독립 격리 (§3.8) |

---

### 5.1 세부 기각 사유 분석 (Deep-Dive Rationales)

#### (1) Rust 코어에 브라우저/CDP/HTML 파서 엔진을 직접 만들지 않는 이유
- **유지보수 비용의 영구적 부채화:** 코드는 작성되는 순간부터 컴파일 오버헤드, 정적 타입 검증, 런타임 패닉 방지, 단위/통합 테스트 유지비, 보안 취약점이라는 영구적인 세금을 물린다.
- **프롬프트/스킬 우선(Prompt-First) 공리 준수:** 상위 지침(프롬프트, 스킬 마크다운)과 기존 CLI 도구의 조합으로 100% 동일하거나 더 뛰어난 인지 성능을 낼 수 있는 문제를 코드로 끌고 들어오는 것은 ADR-0001의 Lean Core 원칙과 `prompt-philosophy.md`의 "프롬프트 우선, 코드 최소성의 법칙"을 정면으로 위배한다. 코어는 엄격한 OS/IO 안전 불변식(Invariants)만 지켜야 한다.

#### (2) 날것의 HTML 덤프를 원천 금지하는 이유
- **16KB 중간 잘림(Middle-Truncation)의 구조적 함정:** `crates/ma-runtime/src/tools.rs`의 `truncate_tool_content`는 16KB 초과 시 앞 3/5(Head)과 뒤 2/5(Tail)만 남기고 본문 가운데를 `[... bytes omitted ...]`로 잘라버린다. 웹 타깃의 핵심인 `<form action="...">`, `<input name="...">`, CSRF hidden 토큰, 로그인 폼은 항상 본문 가운데에 위치하므로, 에이전트가 이를 보지 못해 맹목적 추측(Blind Guessing) 루프에 빠져 전체 턴 예산을 날린다.
- **문맥 80~90% 잠식:** 수만 줄의 Tailwind/Bootstrap CSS 클래스명, 반응형 레이아웃용 `<div>`, 인라인 SVG 그래픽, 웹팩 번들 JS 코드가 수만 토큰을 낭비하여 정작 취약점 분석에 필요한 모델의 추론 문맥을 고갈시킨다.

#### (3) 전역 상시 강제 프록시 방식을 기각하는 이유
- 기존 일부 도구들이 채택하는 상시 강제 라우팅(`ALL_PROXY`)은 타깃 컨테이너 기동 지연 시 프록시 데몬 자체의 502 Bad Gateway HTML 응답을 타깃 서버의 응답으로 에이전트가 오인하는 치명적 환각을 유발한다. 에이전트는 프록시 에러 페이지를 분석하며 엉뚱한 우회 페이로드를 던지느라 경로를 완전히 이탈한다.
- 따라서 프록시는 컨테이너 내에 대기시키되, 에이전트가 패킷 분석이 필요한 순간에만 `export http_proxy=...`를 주입하는 **온디맨드 활성화**가 압도적으로 안전하다.

#### (4) 모든 도구의 전면 비동기화를 기각하는 이유
- 모든 명령을 백그라운드로 실행하면 명령의 결과를 확인하기 위해 다음 턴에 폴링 도구를 다시 호출해야 하므로 턴 수와 토큰이 2~3배 폭증한다.
- 이전 명령의 파일 쓰기나 스캔 완료를 확인하지 않고 다음 공격을 실행하는 인과성 파괴(Causal Breakage)와 파일 경합이 발생하며, 잊혀진 백그라운드 프로세스가 누적되어 컨테이너 OOM을 유발한다.
- 따라서 "95% 동기식 단발 실행(`bash`) + 5% 명시적 장기 세션(`tmux`)"의 실행 이원성(Execution Duality, ADR-0003 §3.5)이 최적의 해법이다.

---

## 6. 시스템 영향 및 기대 효과 (Consequences)

- **안정성:** 브라우저 모달 다이얼로그로 인한 15분 타임아웃 프리징 및 좀비 프로세스 대기 루프 100% 제거.
- **토큰 효율:** 수만 줄의 날것 HTML 대신 ~200토큰 내외의 접근성 트리(`snapshot -i -c`) 및 핀포인트 DOM 조회를 소비하여 문맥 낭비 90% 이상 절감 및 16KB 중간 잘림 방지.
- **아키텍처 무결성:** Rust 코어의 무결성과 단순성을 온전히 보존하면서, 기성 오픈소스 생태계(CLI)를 레고 블록처럼 활용하는 유연성 확보.
