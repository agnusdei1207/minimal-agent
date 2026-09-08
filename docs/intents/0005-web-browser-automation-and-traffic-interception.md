# INTENT-0005 · Web Browser Automation Discipline, CLI Architecture, and On-Demand Traffic Interception

- 상태: 완료
- 작성/갱신 날짜: 2026-09-08

## 왜

에이전트가 웹 타깃을 다룰 때 두 가지 방식이 실전에서 반복적으로 무너졌다. 첫째, 원시 브라우저 바이너리(`google-chrome --headless --dump-dom`, `playwright`, `puppeteer`)를 직접 몰면 `alert()`/`confirm()`/`prompt()` 모달이 자바스크립트와 DOM 렌더링을 완전히 정지시켜(모달 프리즈) 헤드리스 환경에서 브라우저가 영구 멈춤에 빠졌고, 타임아웃으로 자식 쉘이 죽어도 고아 크롬이 `<defunct>` 좀비로 남아 에이전트가 "아직 렌더링 중"으로 오판해 `sleep 60`을 반복하며 15분 이상 시간 예산을 통째로 날렸다(XBEN-013-24). 인스턴스당 300~700MB RAM을 순간 점유해 호스트 OOM-killer의 `SIGKILL`을 부르기도 했다(XBEN-008-24). 둘째, 날것의 HTML(`curl`, `outerHTML`)을 그대로 읽으면 80~90%가 CSS/JS/SVG 쓰레기라 토큰이 폭증하고, 16KB 상한을 넘으면 런타임의 중간 잘림(middle-truncation)이 본문 가운데의 `<form action>`·`<input name>`·CSRF hidden 토큰을 잘라내 에이전트가 핵심을 못 보고 맹목적 추측 루프에 빠졌다.

이 문제가 풀리면 에이전트는 브라우저를 안전하고 토큰 효율적으로 관측·조작하고, 필요할 때만 트래픽을 가로채며, 컨테이너 좀비와 모달 프리즈로 인한 대기 환각 루프에서 완전히 벗어난다. 그리고 이 모든 것을 Rust 코어에 무거운 코드를 한 줄도 더하지 않고 달성한다.

## 무엇

- 에이전트는 표준 `bash` 도구 하나로 경량 CLI(`agent-browser`)를 구동해 웹을 관측·조작하며, 원시 크롬을 직접 실행하려 하면 즉시 가드가 막고 올바른 경로로 안내한다.
- 웹 관측은 날것 HTML이 아니라 컴팩트 접근성 트리(Accessibility Tree, `snapshot -i -c`, ~200토큰)와 필요 노드만의 핀포인트 DOM 조회로 이뤄진다.
- 트래픽 가로채기는 상시 강제 라우팅이 아니라, 패킷 분석이 필요한 순간에만 켜는 온디맨드 방식으로 동작한다.
- 브라우저·서브프로세스의 좀비가 컨테이너에 남지 않으며, 자바스크립트 다이얼로그는 자동 수락되어 프리즈가 발생하지 않는다.
- 최신 도구인 `agent-browser`를 사전학습에서 모르는 모델이라도 매 턴 도구의 존재와 기본 구문을 인지하고, 심층 프로토콜은 필요 시 스킬 카드에서 로드한다.

## 수용 기준

- [x] Rust 코어(`crates/ma-runtime`)에 브라우저 제어 엔진이나 MITM 프록시 엔진이 새로 작성되지 않는다 — 웹/프록시 기능을 위한 Rust 코드 변경은 0줄이다.
- [x] 컨테이너 PATH(`/usr/local/bin`)에서 원시 `google-chrome`/`chromium`을 실행하면 가드 래퍼가 즉시 에러를 내고 `curl`/`agent-browser` 사용을 안내한다.
- [x] `agent-browser snapshot -i -c`가 HTML 태그 없이 Role+Name+State와 `@eN` 식별자만의 초경량 트리를 반환하고, `@eN` 참조로 `click`/`fill`/`press`가 완결된다.
- [x] XSS 페이로드 반영 등 검증이 필요할 때 `agent-browser get html @eN` / `get attr @eN <attr>`으로 해당 노드만 수십 바이트로 조회된다.
- [x] 상시 강제 프록시가 없고, `export http_proxy=http://127.0.0.1:8080`을 일시 주입하는 온디맨드 방식으로만 트래픽이 프록시(`mitmdump`/logging-proxy)를 통과한다.
- [x] 컨테이너가 Docker `--init`(tini PID 1)으로 기동되어 고아 브라우저/서브프로세스가 `<defunct>`로 잔존하지 않고 즉시 회수된다.
- [x] 시스템 프롬프트(`prompts/tradecraft.md`)의 상시 앵커(~40토큰)가 매 턴 `agent-browser`의 존재와 기본 구문을 주입하고, 심층 프로토콜은 `skills/02-web-application.md`에서 로드된다.
- [x] 모든 브라우저 호출이 `--session <agent-id>`로 격리되어 병렬 에이전트 간 페이지 덮어쓰기와 `@eN` 참조 무효화(race condition)가 발생하지 않는다.

## 제약

- **Rust Core Zero-Bloat 불변식:** 코어에 CDP 라이브러리·Playwright 바인딩·HTTP/HTTPS MITM 엔진을 새로 넣지 않는다. 모든 외부 도구 조작은 표준 `bash`/`tmux`로 CLI 수준에서 완결한다(INTENT-0001 최소 코어, INTENT-0003 쉘 실행 모델).
- **프롬프트 우선(Prompt-First):** 프롬프트·스킬과 기성 CLI 조합으로 동일하거나 더 나은 인지 성능을 낼 수 있는 문제를 코드로 끌어오지 않는다. 코어는 엄격한 OS/IO 안전 불변식만 지킨다.
- **관측 규약:** 날것 HTML 덤프(`outerHTML`, 무차별 `curl`)를 금지한다. 기본 관측은 접근성 트리(`snapshot -i -c`)이며, 문서성 사이트는 `w3m -dump -cols 120` 또는 `html2text`로, 정적 크롤링은 BeautifulSoup 원라이너·정밀 `grep`으로 `<form>`·`<input>`·주석만 좁혀 본다(토큰 80~90% 절감, 16KB 중간 잘림 회피).
- **SPA 안정화:** 페이지 이동 직후 빈 껍데기 관측을 막기 위해 `agent-browser wait --load networkidle`을 규약으로 둔다. DOM 변경 시 `snapshot -i -c`를 재호출해 stale ref를 방지한다.
- **세션 수명:** 컨테이너 데몬 `IDLE_TIMEOUT=180000`(3분 유휴 자동 종료)을 전제로, 장시간 오프라인 작업 후 세션 초기화 함정을 가이드로 회피한다.
- **온디맨드 프록시 + 원시 탭:** 프록시는 대기시키되 필요 시에만 `http_proxy`로 주입한다. 컨테이너에 `NET_RAW`/`NET_ADMIN`(`docker/compose.yaml`)을 부여해 무거운 프레임워크 없이 `tcpdump`/`scapy`/`tshark`로 OS 네이티브 원시 네트워크 탭과 양방향 패킷 조작을 즉시 수행한다.
- **이중 주입(Dual Injection):** 상시 앵커(시스템 프롬프트 ~40토큰)로 도구 발견성 100%를 보장하고, 심층 방법론(SPA 로드 대기, DOM 정밀 검증, `w3m`/`html2text` 파이프라인)은 온디스크 스킬 카드로 미룬다.
- **컨텍스트 격리(INTENT-0004 연동):** 대용량 웹 정찰은 Worker(leaf)가 전담하고 그 터미널 출력은 Main 문맥을 오염시키지 않는다. Worker는 `Insight`/`Progress` 정형 메시지로 정제된 단서와 원시 값(예: `/login` 파라미터 `user`,`pass`)만 상위로 verbatim 전달한다.
- **실행 이원성(INTENT-0003 연동):** "95% 동기식 `bash` + 5% 명시적 `tmux`" 모델을 유지한다. `bash`의 `timeout_secs` 자율권으로 `nmap`/`ffuf`/`dirb` 등 고지연 정찰의 정상 실행을 보장한다.

## 비범위

- Rust 코어 내 브라우저/CDP/HTML 파서·MITM 엔진 자체 개발.
- 별도 브라우저 함수 도구(`browser_click`, `browser_type` 등) 스키마 신설.
- 전역 상시 강제 프록시(`ALL_PROXY`) 라우팅.
- 모든 도구의 전면 백그라운드/비동기 호출 모델.
- 쉘 전역 타임아웃 일괄 단축(예: 30초 고정).

## 열린 질문 → 결정

- 결정: **브라우저 자동화는 Rust 코어가 아니라 CLI+스킬 프롬프트가 주도한다.** 기성 `agent-browser` CLI를 표준 `bash`로 구동하고 관측/행동 규약을 `prompts/skills/02-web-application.md`로 통제한다 — 코드는 영구 부채(컴파일·타입·패닉 방지·테스트 유지·보안)이며, 프롬프트로 100% 해결 가능한 문제를 코드로 가져오는 것은 INTENT-0001 Lean Core 위배이기 때문.
- 결정: **날것 HTML 대신 접근성 트리를 기본 관측 단위로 삼는다.** `snapshot -i -c`(Role/Name/State + `@eN`)로 정제하고 필요 노드만 핀포인트 조회 — CSS/JS/SVG가 문맥 80~90%를 잠식하고 16KB 중간 잘림이 Form/Input/CSRF 토큰을 삭제해 맹목적 추측 루프를 유발하기 때문.
- 결정: **트래픽 가로채기는 온디맨드로 제한한다.** 상시 강제 프록시는 타깃 미도달 시 프록시 데몬의 502 Bad Gateway를 타깃 응답으로 오인하는 치명적 환각을 일으키므로 기각하고, 패킷 분석이 필요한 순간에만 `export http_proxy`를 주입한다.
- 결정: **원시 크롬 실행을 가드 래퍼로 차단하고 다이얼로그 자동 수락 CLI를 강제한다.** 원시 브라우저의 `alert()` 모달 프리즈(15분 프리징)와 `<defunct>` 좀비 착시, 300~700MB RAM OOM을 원천 차단하기 위함.
- 결정: **좀비는 Docker `--init`(tini PID 1)으로 회수한다.** `benchmarks/harness/runner.mjs` 및 모든 컨테이너 기동에 `--init`을 강제해 고아 프로세스를 0ms 내 reap하여 `<defunct>` 대기 루프를 영구 제거.
- 결정: **도구 발견성은 이중 주입으로 보장한다.** GLM/DeepSeek 등 사전학습에 `agent-browser`가 없으므로 순수 지연 로딩(스킬만) 의존을 기각하고, `tradecraft.md`에 ~40토큰 상시 앵커 + 스킬 카드 심층 프로토콜을 병용한다.
- 결정: **병렬 에이전트는 `--session <agent-id>`로 브라우저를 격리한다.** 단일 컨테이너 내 최대 10 에이전트의 세션 공유가 페이지 덮어쓰기·`@eN` 무효화 race condition을 일으키므로 모든 호출에 세션 플래그를 의무화.

---

## AI 판정

| 수용 기준 | 증거 | 판정 |
| :--- | :--- | :--- |
| Rust 코어 Zero-Bloat (웹/프록시 코드 0줄) | 접근성 트리·관측 파이프라인을 Rust 런타임 코드 없이 시스템 프롬프트(`prompts/skills/02-web-application.md`)로만 배선; 코어 수정 0줄 유지 | 통과 |
| 원시 크롬 차단 가드 | PATH(`/usr/local/bin`)의 `google-chrome`/`chromium` 심볼릭 링크 제거, 즉시 에러 출력 가드 래퍼로 대체 배선 | 통과 |
| 접근성 트리 관측 + `@eN` 제어 | `snapshot -i -c`가 Role/Name/State+`@eN` 초경량 트리 반환, `click @e3`/`fill @e2`/`press Enter` 완결 규약 확립 | 통과 |
| 핀포인트 DOM 조회 | `get html @eN`/`get attr @eN <attr>`로 필요 노드만 수십 바이트 조회 규약 확립 | 통과 |
| 온디맨드 프록시 | 상시 강제 라우팅 기각, `mitmdump`/`scripts/logging-proxy.py` 온디맨드 + `export http_proxy` 일시 주입 레시피 반영 | 통과 |
| Docker `--init` 좀비 회수 | `benchmarks/harness/runner.mjs` 및 모든 컨테이너 기동에 `--init` 강제, tini가 PID 1로 0ms reap | 통과 |
| 이중 주입 도구 발견성 | `prompts/tradecraft.md` BROWSER AUTOMATION ~40토큰 상시 앵커 + `skills/02-web-application.md` 심층 프로토콜 반영 | 통과 |
| `--session` 세션 격리 | 모든 브라우저 호출에 `--session <agent-id>` 의무화로 독립 브라우징 컨텍스트 격리 | 통과 |

**구조 지도 변경:** 코드 구조 변경 없음(Zero Runtime Code). 웹 관측/트래픽 규약은 `prompts/tradecraft.md`(상시 앵커)와 `prompts/skills/02-web-application.md`(심층 프로토콜)에 살고, 인프라 가드는 Docker `--init`·PATH 가드 래퍼·`docker/compose.yaml`의 `NET_RAW`/`NET_ADMIN`으로 존재한다.

**남은 것:** 없음(인프라·안전 가드·접근성 트리·온디맨드 프록시 전 계층 반영 완료). 규약이 프롬프트/스킬 주도이므로 향후 유지보수는 코어가 아닌 프롬프트·스킬 카드 층에서 이뤄진다.

**남는 위험:** `agent-browser`·`snapshot -i -c` 등 최신 CLI가 사전학습에 없는 신규 모델이 도입되면 이중 주입 앵커가 흐려질 때 과거 Selenium/원시 크롬 회귀 함정이 재발할 수 있다 — 상시 앵커 유지가 방어선이다. 온디맨드 프록시는 에이전트가 `http_proxy` 해제를 잊으면 부분적 상시 라우팅 부작용이 남을 수 있어 스킬 프롬프트의 주입/해제 규율에 의존한다.

**발견한 부채:** `benchmarks/harness/Dockerfile.runner`의 중복 패키지 설치 레이어(`netcat-openbsd`/`iputils-ping`는 이미 `docker/runtime-base.Dockerfile`에 포함) 제거로 빌드 시간·디스크 절감. PATH 상 원시 크롬 노출과 Rust 코어 내 브라우저/프록시 드라이버 작성 시도는 안티패턴으로 공식 배제.
