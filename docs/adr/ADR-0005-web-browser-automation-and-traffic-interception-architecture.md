# ADR-0005: Web Browser Automation Discipline, CLI Architecture, and On-Demand Traffic Interception

- Status: Accepted (반영 완료 · 스킬 프롬프트 주도 아키텍처)
- Created: 2026-09-06 +09:00
- Version target: 0.111.0
- Repository: `agnusdei1207/minimal-agent`
- Relation: ADR-0001(Anti-Bloat Clean-Room Core), ADR-0002(Authorized Engagement), ADR-0003(Shell & Interactive Terminal Primitives), ADR-0004(Bounded Three-Depth Hierarchy).
- Background Design Doc: [`docs/design/browser-automation-trap-and-xss-execution-discipline.md`](../design/browser-automation-trap-and-xss-execution-discipline.md)
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
- **정적 HTTP/크롤링 위생:** `curl`을 쓸 때도 HTML 전체를 표준 출력에 쏟아내지 않고, BeautifulSoup 원라이너나 정밀 `grep`을 파이프로 연결해 `<form>`, `<input>`, 주석(`<!-- -->`)만 좁혀서 관측하도록 스킬 프롬프트에 명시.

### 3.4 온디맨드 트래픽 가로채기 (On-Demand Proxy, No Full-Time Forcing) [반영 완료]
- 외부 도구(Strix 등)의 상시 강제 프록시(`ALL_PROXY`) 구조는 타깃 서비스 미기동 또는 일시 장애 시 프록시 데몬 자체의 502 Bad Gateway HTML 응답을 타깃 서버의 응답으로 에이전트가 오인하는 치명적 환각 부작용을 유발한다.
- 따라서 `minimal-agent`는 **상시 강제 라우팅을 명시적으로 기각**한다.
- 컨테이너에 기설치된 `mitmproxy`(`mitmdump`) 또는 `caido-cli`는 데몬 형태로 대기하며, 패킷 분석이 필요한 특정 단계에서만 에이전트가 `export http_proxy=http://127.0.0.1:8080`을 설정하여 온디맨드로 활용한다.

### 3.5 컨테이너 좀비 프로세스 자동 회수 (`--init` 강제) [반영 완료]
- `benchmarks/harness/runner.mjs` 및 모든 에이전트 컨테이너 기동 명령에 Docker `--init` 플래그를 강제했다.
- Docker 내장 경량 init(`tini`)이 PID 1을 맡아, 충돌하거나 비정상 종료된 브라우저/서브프로세스의 고아 프로세스를 0ms 내에 완전히 회수(reap)하여 `ps aux` 상의 `<defunct>` 좀비 착시 루프를 영구 제거했다.

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

| 대안 | 기각 사유 |
| :--- | :--- |
| **Rust 코어에 CDP / 프록시 엔진 직접 구현** | 수천 줄의 복잡한 네트워크/비동기 코드가 유입되어 ADR-0001의 Lean Core 원칙 파괴. 유지보수 비용 폭증. |
| **전역 상시 강제 프록시 (Strix 방식)** | 타깃 미도달 시 프록시 자체의 502/500 에러 페이지를 타깃의 응답으로 에이전트가 환각하는 부작용 확인. |
| **쉘 전역 타임아웃 단축 (예: 30초로 제한)** | `nmap`, `ffuf`, `dirb` 등 정상적인 고지연 대용량 정찰 도구들의 정상 실행을 방해함. 대신 ADR-0003 §3.4에 따라 `bash`의 동적 `timeout_secs` 파라미터로 에이전트 자율성에 위임함. |
| **로컬 Puppeteer/Playwright 스크립트 작성 유도** | LLM이 프레임워크 스크립트 작성 중 잦은 문법 실수와 비동기 프로미스 처리 오류를 범하여 루프에 빠짐. |

---

## 6. 시스템 영향 및 기대 효과 (Consequences)

- **안정성:** 브라우저 모달 다이얼로그로 인한 15분 타임아웃 프리징 및 좀비 프로세스 대기 루프 100% 제거.
- **토큰 효율:** 수만 줄의 날것 HTML 대신 ~200토큰 내외의 접근성 트리(`snapshot -i -c`) 및 핀포인트 DOM 조회를 소비하여 문맥 낭비 90% 이상 절감 및 16KB 중간 잘림 방지.
- **아키텍처 무결성:** Rust 코어의 무결성과 단순성을 온전히 보존하면서, 기성 오픈소스 생태계(CLI)를 레고 블록처럼 활용하는 유연성 확보.
