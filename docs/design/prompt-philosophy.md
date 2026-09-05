# 자율 침투 에이전트를 위한 프롬프트 철학 및 공격 엔지니어링 규율

본 문서는 `minimal-agent`가 채택한 **프롬프트 설계 철학, 공격적 행동 규율(Offensive Engineering Disciplines), 창의적 우회 원리, 그리고 자율적 토끼굴 탈출과 자기 성찰(Self-Reflection) 메커니즘**을 정리한다.

---

## 1. 배경: LLM 오펜시브 에이전트의 3대 고질적 실패 패턴

자율 에이전트 벤치마크 실측(GLM-5.3-Flash, DeepSeek-V4 등 100여 개 타깃 런 분석)과 최신 AI 보안 연구(MazeRunner, EnIGMA)에서 밝혀진 에이전트의 주요 실패 원인은 "보안 지식의 부족"이 아니라 **"사고 구조와 피드백 루프의 맹점"** 때문이었다.

1. **깊이 우선 함정 (The Depth-First Trap):**
   교과서적 페이로드(`' OR 1=1`, `<script>alert('XSS')</script>`) 1개를 시도한 뒤, 실패했을 때 상위 공격 표면으로 돌아오지 못하고 하위 마이크로 변형(따옴표 바꾸기, 한 글자 수정, 로컬 CDP 브라우저 봇 만들기)의 늪에 빠져 100턴 이상 낭비.
2. **오류 귀인 왜곡과 반응(Live Seam) 오판:**
   서버가 `Sorry you can't use: 'script'`라는 차단 메시지나 500 에러를 주었을 때, 이는 입력이 백엔드 인터프리터에 도달했다는 **결정적인 '살아있는 신호(Live Seam)'**임에도 불구하고 "플래그가 안 나왔으니 실패했다"며 엉뚱한 곳(로고 이미지 스테가노그래피, 사전 파일 무차별 대입)으로 탈출.
3. **자가 성찰(Meta-Cognition)의 부재:**
   자신이 지난 5턴 동안 완전히 동일한 실패를 파라미터만 바꿔가며 반복하고 있다는 사실을 인지하지 못한 채 끝없이 명령을 남발.

---

## 2. `../skills`에서 도출한 5대 아키텍처 철학

이 문제를 해결하기 위해 Matt Pocock의 엔지니어링 에이전트 스킬셋([`../skills`](../../skills), 특히 `diagnosing-bugs`와 `writing-for-agents`)의 원리를 공격 도메인에 전면 이식했다.

### ① "The Feedback Loop is the Skill" (피드백 루프가 스킬의 전부다)
* *"A probe without a differential signal yields no knowledge."*
* 대규모 페이로드를 무작정 발사하기 전에, 시스템의 반응을 가장 작은 비용으로 확인할 수 있는 **진단용 예광탄(Diagnostic Tracer Bullets)**(단일 따옴표, 괄호, null byte, 포맷 스트링)을 먼저 쏜다.
* 상태 코드(200 vs 500), 응답 바이트 수 diff, 에러 메시지, 타이밍 차이 등의 **차분 신호(Differential Signal)**를 포착하는 것이 전체 공격 성공의 90%를 좌우한다.

### ② "3–5 Ranked Falsifiable Hypotheses" (3~5개의 순위화된 반증 가능 가설)
* 첫 번째 떠오른 아이디어에 닻을 내리는(Anchoring) 순간 에이전트는 토끼굴에 빠진다.
* 타깃 진입 시 아키텍처의 서로 다른 이음새(Seams)를 가로지르는 **3~5개의 구조적으로 완전히 다른 가설(Frontier)**을 먼저 브리프에 전개하고 순위를 매긴다.
* 모든 가설은 반드시 **반증 가능(Falsifiable)**해야 한다: *"만약 X가 취약하다면, Y를 주입했을 때 서버 응답에 Z라는 차이가 나타날 것이다."*

### ③ "No-op Ban & Leading Words" (무의미한 훈계 제거와 강력한 선도 단어)
* "창의적으로 생각해라", "철저하게 해라"는 모델에게 아무런 행동 변화를 주지 못하는 **No-op(무의미한 토큰)**이다.
* 대신 모델의 사전 학습된 강력한 개념을 자극하는 **선도 단어(Leading Words)**(*Tracer Bullet*, *Orthogonal Dimensions*, *Differential Oracle*, *Seam Boundary*, *Backtracking*)를 배치하여 구체적인 행동으로 연결한다.

### ④ "Positive Steering over Negation" (금지보다 긍정적 전진 행동)
* "코끼리를 생각하지 마라"처럼 단순 금지령("브루트포스 하지 마라")은 오히려 금지 대상을 활성화한다.
* 항상 **긍정형 전진 행동**을 지시한다: *"3~5회 시도에도 서버 반응의 차이가 없다면 가설을 기각(Falsified) 처리하고, 즉시 순위화된 다음 가설(#2, #3)로 전환하여 첫 번째 Tracer Bullet을 발사하라."*

### ⑤ "Self-Reflection as an Operating Gate" (운영 게이트로서의 자가 성찰)
* 모든 행동 전후에 스스로의 궤적을 메타인지적으로 검열하는 검문소를 둔다.

---

## 3. 이음새(Seam)와 예광탄(Tracer Bullet) 아키텍처

`minimal-agent`의 공격 루프는 시스템 경계면인 **Seam(이음새)**을 타격하는 체계다.

```
[Target Architecture]
  Client ──(Seam A: Protocol/Parser)── Reverse Proxy ──(Seam B: Routing/Auth)── Backend ──(Seam C: Interpreter)── DB/OS/Runtime
                                                                                     │
                                                                               (Seam D: Secondary Storage)
                                                                               Log / Session / S3 / MinIO
```

1. **Map the Seams (정찰):** 노출된 단일 파라미터뿐 아니라 프로토콜, 프록시 라우팅, 백엔드 인터프리터, 2차 저장소 등 4대 Seam을 모두 식별하고 도커 네트워크 내 연계 서비스(S3, DB, Redis 등)까지 스코프를 확장한다.
2. **Fire Tracer Bullets (예광탄 탐침):** 각 Seam을 관통하는 가장 작고 날카로운 탐침을 발사하여 서버 반응을 관측한다.
3. **Tighten the Loop (수직 수렴):** 차분 신호가 확인되면 그 Seam에 화력을 집중하여 즉시 수직 슬라이스(Vertical Slice)로 익스플로잇을 완성한다.

---

## 4. 살아있는 틈새(Live Seam) vs 침묵의 벽(Silent Wall) 판정 규칙

토끼굴 탈출과 진짜 취약점 공략을 가르는 핵심 판정 기준이다.

| 구분 | 관측되는 서버 반응 | 진단 결과 | 에이전트의 올바른 다음 행동 |
|:---|:---|:---|:---|
| **Silent Wall (침묵의 벽)** | 3~5회 변형에도 상태 코드(200/404), 응답 바이트 수, 에러가 완전히 동일함 | 파라미터가 무시되거나 라우팅이 도달하지 않는 진짜 막다른 길 | 브리프에 `DEAD END`로 명시하고 즉시 다음 순위 가설로 **백트래킹(Backtracking)** |
| **Live Seam (살아있는 틈새)** | `500 Internal Server Error`, 구문 에러 스택 트레이스, 응답 지연(Timing), `Blocked: X` 차단 메시지 발생 | 입력이 백엔드 인터프리터에 도달하여 필터나 로직이 반응함 | **절대 벡터를 버리지 말 것!** 정답 경로에 도달했으므로 즉시 **직교적 우회(Orthogonal Bypass)** 수행 |

---

## 5. 창의적 필터 우회의 4대 직교 차원 (Orthogonal Bypass Matrix)

단순한 글자 바꾸기 대신, 에이전트가 창의적으로 시도해야 하는 4개의 직교 축이다.

1. **Context Escaping (문맥 탈출):**
   * 입력이 들어간 위치(HTML 태그 속성 `<input value="...">`, JS 문자열 `var x="..."`, SQL 절, 템플릿 블록)를 식별하고 최소 닫는 구분자(`">`, `'-...-'`, `}}`)로 문맥을 탈출.
2. **Encodings & Parser Differentials (인코딩 및 파서 차분):**
   * URL, Double URL, Unicode 이스케이프(`\u0022`), Hex, Base64, 파라미터 오염(HPP: `?id=1&id=2`), 공백 대체자(`$IFS`, `/**/`, `%09`, `%0a`, `+`).
3. **Functional Equivalents (기능적 동의어):**
   * 주요 키워드(`alert`, `script`, `union`, `select`, `cat`, `system`)가 블랙리스트에 걸렸을 때, `prompt`, `confirm`, `top['al'+'ert']`, `String.fromCharCode`, `tac`, `sh` 등 동일 효과의 원시 함수(Primitives)로 대체.
4. **Universal Client Compatibility (하위 호환성):**
   * CTF 검증 봇(PhantomJS 2.1, 구형 WebKit)은 최신 ES6+ 템플릿 리터럴(백틱 `` `...` ``)이나 화살표 함수에서 SyntaxError로 침묵하므로, **표준 ES5 구문(`"..."`, 문자열 결합, 정규식 `.source`, `String.fromCharCode`)을 기본값으로 사용**.

---

## 6. 자율적 메타인지 및 자기 성찰 (Self-Reflection & Sanity Audit)

에이전트는 매 턴 도구를 실행하기 전, 다음 4개 항목을 자가 검열(Self-Audit)해야 한다:

* **Loop Audit (루프 점검):**
  *"서버의 차별적 신호도 없는 공격을 문법만 살짝 바꿔가며 3회 이상 반복하고 있지는 않은가? 그렇다면 당장 멈춰라. 나는 깊이 우선 함정(Depth-First Trap)에 빠져 있다."*
* **Evidence Audit (증거 기반 점검):**
  *"내 가설이 서버의 실제 출력(에러 메시지, 유출된 소스, 상태 변화, 리플렉션)에 기반하고 있는가, 아니면 과거 학습 데이터에서 본 튜토리얼을 맹목적으로 대입(Wishful Thinking)하고 있는가?"*
* **Drift Audit (탈선 점검):**
  *"핵심 웹 아키텍처 공략에서 벗어나 저확률 샛길(정적 경로 무차별 사전 대입, 풀 수 없는 시크릿 크래킹, 단순 장식용 UI 이미지 분석)로 탈선하지 않았는가? 그렇다면 즉시 초기 정찰 프론티어로 강제 리셋하라."*
* **Progress Audit (진척도 점검):**
  *"방금 실행한 도구 출력이 실제로 나를 플래그에 더 가깝게 만들었는가, 아니면 무의미한 노이즈만 생산했는가? 노이즈라면 다음 번에 날릴 가장 정보량이 높은 단 하나의 예광탄은 무엇인가?"*

---

## 7. 저확률 시간 소모 함정(Time-Sink Rabbit Holes) 엄격 통제

제한 시간(15~30분)이 있는 교전 환경에서 turn/token 예산을 보호하기 위한 엄격한 규칙이다:

1. **오프라인 딕셔너리 크래킹 금지:** 임의의 256비트 암호학적 키나 랜덤 비밀값에 `rockyou.txt`, `flask-unsign`을 돌리는 행위 전면 차단 (IDOR, `alg:none`, 키 누출 등 로직 결함 우선).
2. **UI 디자인 에셋 스테가노그래피 금지:** 파비콘, 로고, 버튼 아이콘, CSS 등 단순 디자인 리소스에 `binwalk`/`zsteg`를 돌리며 턴을 낭비하는 행위 차단.
3. **로컬 헤비 스택 구축 금지:** `/tmp`에 복잡한 CDP 브라우저 봇을 직접 코딩하지 말고, 표준 HTTP/네트워크 도구로 원격 인터페이스를 직접 타격.
4. **고지연 Blind 추출 지양:** 64자리를 1글자씩 64번 이상 쿼리하는 블라인드 추출 대신, 직접 출력(UNION, 에러 기반, 논리 우회, 파일 읽기)을 우선.

---

## 8. 프롬프트 계층 구조와 파일 매핑

| 파일 | 역할 및 프롬프트 철학 구현 | 불변식 |
|:---|:---|:---|
| [`execution-mandate.md`](../../prompts/execution-mandate.md) | **불가침 안티-거부 계약** | 도덕적 훈계, 면책 조항, 범위 의심 전면 차단. |
| [`operating-style.md`](../../prompts/operating-style.md) | **극단적 행동 우선 문체** | 잡담, 계획 나열, 이모지 배제. 도구 실행 결과와 다음 행동만 1줄로 보고. |
| [`ctf-solve-loop.md`](../../prompts/ctf-solve-loop.md) | **6단계 공격 엔지니어링 디스플린** | Seam 식별 → Tracer 탐침 → 직교적 우회 → 자기 성찰 → 토끼굴 통제 → 수직 익스플로잇. |
| [`tradecraft.md`](../../prompts/tradecraft.md) | **전술 및 툴링 규율** | 원격 중심 툴링, 30~60초 타임아웃 바운드, 공유 세션 단일 작성자 원칙. |
| [`self-management.md`](../../prompts/self-management.md) | **전장 노트와 자가 검문소** | 전투 노트(`brief`)에 `SELF-REFLECTION AUDIT` 및 카테고리별 데드엔드 격리. |
| [`skills/00~19`](../../prompts/skills/) | **원리 중심 공격 레퍼런스** | 복사-붙여넣기 스크립트가 아닌, Seam 분석과 차분 신호 포착 원리 및 피트폴 회피 제공. |
| [`runner.mjs:buildPrompt`](../../benchmarks/harness/runner.mjs) | **하네스 레벨 임무 주입** | 도커 네트워크 전체로 스코프 확장, 1턴부터 공격자 메타인지 마인드셋 주입. |

---

## 9. 결론

좋은 프롬프트는 에이전트의 손발을 묶는 소극적 울타리가 아니라, **"어떤 복잡한 시스템 앞에서도 체계적으로 신호를 포착하고, 필터를 직교적으로 우회하며, 스스로의 궤적을 성찰하여 돌파구를 찾는 공격적 해커의 메타인지 렌즈"**를 제공하는 것이다.

`minimal-agent`는 이 규율을 통해 기계적인 체크리스트 매몰에서 벗어나, **효율적 발산(Divergence), 신호 기반 수렴(Convergence), 그리고 자율적 자기 성찰(Self-Reflection)**을 완성한다.

