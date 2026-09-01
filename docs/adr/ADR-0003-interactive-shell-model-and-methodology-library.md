# ADR-0003: Interactive Shell Model and Autonomous Methodology Library

- Status: Proposed
- Created: 2026-09-01 +09:00
- Version target: 0.111.0 (ADR-0002와 함께 릴리스)
- Repository: `agnusdei1207/minimal-agent`
- Relation: ADR-0001을 대체하지 않고 확장한다. ADR-0001의 flat 2-depth topology,
  journal/brief 소유권, 최소 도구·자원 상한, provider 불변식과 ADR-0002의 승인된 교전
  doctrine은 그대로 유지한다. 본 ADR은 ADR-0001 §14가 범위 밖으로 둔 "터미널
  에뮬레이터 재구현"과 구분되는 **대화형/지속 세션의 운영 모델**과, 약한 모델도
  방향을 잡게 하는 **자율 참조용 공격 방법론 라이브러리**만 다룬다.
- Exploration note: `docs/design/interactive-shell-ideas.md`가 이 결정의 탐색 노트다.

## 1. 한 문장 결정

대화형·지속 세션은 **코어에 넣지 않고 "공용 책상(shared named session)"으로** 둔다 —
여러 에이전트가 하나의 tmux 세션에 직접 붙어 쓰되 **한 번에 한 명만** 타이핑하고
조종간을 팻말처럼 넘긴다. 그리고 약한 모델도 낯선 대상에서 방향을 잡도록, **원리
수준의 공격 방법론 카드를 디스크에 실어** 에이전트가 자율적으로 찾아 읽게 한다.
소프트웨어가 강제하는 건 소수의 안전 레일뿐이고, 나머지는 전부 에이전트 자율이다.

## 2. 배경과 문제

리버스 셸을 `nc`로 받으면 "덤프 셸"이다 — 제어 tty가 없어 job control·`sudo`·에디터·
전체화면 프로그램·프롬프트가 깨진다. 전통적 "full TTY 업그레이드"는 로컬 터미널을
사람이 직접 raw 모드로 돌리는(`Ctrl-Z` → `stty raw -echo` → `fg`) 순차 조작을 요구하는데,
이는 **이산 턴으로 도구를 호출하는 LLM**에게는 표현하기 어렵다.

두 가지를 동시에 풀어야 한다.

1. **대화형/지속성:** 코어(터미널 에뮬레이터·PTY 데몬·세션 IPC)를 키우지 않으면서
   대화형 세션과 리버스 셸 유지를 어떻게 지원하는가?
2. **약한 모델의 방향성:** 능력이 약한 모델이 낯선 대상 유형(AD, pwn, 크립토,
   임베디드/로봇 …)을 만났을 때, 대본 없이도 어디를 파고들지 어떻게 알게 하는가?

## 3. 결정

### 3.1 대화형 세션 = 공용 책상 (Shared Session)

- 대화형/지속 세션은 **이름 붙은 tmux 세션 하나**로 표현한다. 이 세션은 특정
  에이전트의 소유물이 아니라 **팀이 공유하는 자원(책상)**이다.
- 어떤 에이전트든 그 세션에 **직접 붙어(attach) 타이핑하고, 화면을 직접 읽는다.**
  화면 내용을 팀 메시지로 복사해 중계하지 않는다(§3.5의 배선 이유).
- **조종간(control)은 넘긴다.** "지금 내가 잡음/넘김"을 팀 메시지로 알리며 자리를
  교대한다. 누가·언제 넘길지는 에이전트 자율이며 코어가 대본을 강제하지 않는다.
- **단일 기록자 불변식:** 한 세션에는 동시에 한 명만 쓴다. 두 타이피스트가 한 세션에
  입력하면 스트림이 섞여 손상된다 → 이 한 가지는 **소프트웨어가 강제**한다(§3.4).

### 3.2 리버스 셸 업그레이드: 리스너를 공용 책상 안에서

로컬 raw-mode 춤이 필요한 유일한 이유는 리스너가 "덤프 끝"이기 때문이다. **리스너를
공용 세션(=이미 실제 터미널) 안에서 띄우면** 로컬 raw 조작이 사라지고, 원격에만 pty를
붙이면 된다: 원격 `pty.spawn` → `TERM`·`stty rows/cols` 정합. 시그널은 세션에 키를
직접 주입해 전달한다(로컬 `stty raw` 불요). `socat`이 대상에 있으면 완전한 pty를 한
번에 넘길 수 있다.

### 3.3 비대화형 우선: `shell.stdin` 프리미티브

가장 싼 길은 애초에 대화형을 피하는 것이다. `shell` 도구에 **`stdin` 옵션 필드 하나**를
더한다(파이프 열고 쓰고 EOF). `sudo -S`, 사전 구성 질의응답 일괄 전송, 인라인 스크립트
원샷을 지속 세션 없이 처리한다. one-shot·stateless·결정론 불변식(ADR-0001 §12)을 그대로
보존한다. **이것이 코어에 추가하는 유일한 도구 표면이다.**

### 3.4 소프트웨어가 강제하는 안전 레일 (최소)

에이전트 자율이 원칙이나, "잊으면 위험한" 소수만 런타임이 보장한다.

- **단일 기록자:** 공용 세션당 동시 1인 기록. 나머지는 붙어서 읽기만.
- **자동 정리(쓰기-정리 비용):** run 종료 시 이 run이 연 세션·리스너를 결정적으로
  정리한다. 세션명은 **run-id로 네임스페이스**(`ma-<runid>-<name>`)하여 재개 시 자기
  것만 재클레임하고, 고아 세션을 식별·수확한다. tmux 세션이 프로세스보다 오래 살아도
  주인 없는 라이브 리스너가 남지 않게 한다(ADR-0001 kill-on-drop 정합).
- **자원 상한:** 동시 세션 수·리스너 수명·relisten 시도(지수 backoff)에 상한을 둔다.
- **버려진 팻말 회수:** 기록자가 사라지면 일정 무활동 후 세션의 기록권이 자동 해제된다.

### 3.5 세션 IPC를 팀 버스로 오해하지 않기 (배선)

캡처 출력은 최대 128 KiB이나 팀 메시지 body는 4 KiB(ADR-0001 §6.3)다. 따라서 화면을
메시지로 중계하면 **경계에서 잘려 유실**된다. 그래서 §3.1은 "각자 세션에서 직접 읽기"로
정한다. 굳이 결과를 넘겨야 하면 workspace 파일로 쓰고 포인터만 메시지로 보낸다. 즉
**팀 버스는 조종간 교대·신호용, 대용량 화면은 세션/파일**이다.

#### 3.6 자율 참조용 공격 방법론 라이브러리 (19종 카드)

- 원리 수준(대본 아님)의 **공격 방법론 카드**를 저장소 `prompts/skills/`에 두고, 런타임
  이미지의 `/opt/minimal-agent/skills`로 실어 나른다. 프롬프트에 굽지 않는다 —
  에이전트가 **필요한 카드만 골라 읽어** 턴 프롬프트를 작게 유지한다(최소성).
- **국제 대회급/실전 공격 커버리지 (19개 분과, 01~19):**
  - `01` 정찰·열거 (Vhost/포트/DNS/API 스키마)
  - `02` 웹 애플리케이션 (Prototype Pollution, SSTI, Deserialization, SSRF, JWT)
  - `03` 네트워크 스니핑 & MITM (L2/L3 스푸핑, NTLM 릴레이, Scapy)
  - `04` 패스워드 & 자격증명 공격 (Hashcat/John 룰 변소, Kerberoast/AS-REP)
  - `05` 리눅스 권한 상승 (SUID/Capabilities, Sudo 탈출, Kernel, Cgroup/Docker 탈출)
  - `06` 윈도우 & Active Directory (BloodHound 그래프, ADCS ESC1-8, RBCD, Token 사칭)
  - `07` 리버스 셸 & 후속 침투 (공용 세션 리스너, PTY 업그레이드, Ligolo-ng 피보팅)
  - `08` 바이너리 익스플로잇 (Stack ROP/SROP, glibc Safe Linking/FSOP House of Apple, Kernel eBPF/modprobe, JIT)
  - `09` 리버스 엔지니어링 (Z3 SMT Solver 제약식 자동 역산, angr 기호 실행, VM 난독화 해제)
  - `10` 암호학 공격 (격자 Lattice LLL/Coppersmith/CVP, ECDSA Nonce 편향 HNP, RSA, Smart's attack)
  - `11` 임베디드 & 펌웨어 (Binwalk/UBIFS 언팩, QEMU 에뮬레이션, 하드코딩 비밀, UART/JTAG)
  - `12` 로보틱스 / ICS / OT (Modbus/S7/CIP/DNP3 레지스터 조작, unauthenticated ROS1/2 토픽)
  - `13` 클라우드 & 컨테이너 (AWS/GCP IMDS & IAM 정책 권한상승, K8s RBAC/ServiceAccount, 컨테이너 탈출)
  - `14` 무선 & RF (802.11 WPA2/Enterprise 핸드셰이크/PMKID, BLE GATT 조작, SDR 역공학)
  - `15` HTTP 인터셉트 & 재전송 (HTTP Request Smuggling CL.TE/TE.CL/H2, 단일 패킷 레이스 컨디션)
  - `16` 포렌식 (Volatility 3 메모리 분석, USB HID PCAP 패킷 역파싱, NTFS MFT/EVTX)
  - `17` 스테가노그래피 (Zsteg 비트 플레인 LSB, DCT 주파수 도메인, 오디오 스펙트로그램)
  - `18` 스마트 컨트랙트 & Web3 (Solidity 재진입성, 플래시론 오라클 조작, 프록시 스토리지 충돌, Foundry)
  - `19` AI & LLM 보안 (Direct/Indirect Prompt Injection, PyTorch/Pickle 모델 RCE, 에이전트 도구 하이재킹)
- doctrine 한 줄(`prompts/tradecraft.md`)이 라이브러리 위치·자율 사용 규칙·공용 세션
  규칙을 알린다. **어떤 카드를 볼지는 에이전트의 판단**이며 고정 절차가 아니다.
- 모든 카드는 clean-room(이 저장소용 저술). `../pentesting`에서 복사하지 않는다.
- **떠먹이지 않되 최신 공격 프리미티브는 정확하게 제시:** 카드와 doctrine은 단순한 구형 교과서
  요약이 아니라, **현대 국제 CTF 및 최신 보안 점검에서 통하는 공격 축(Attack Primitives)과
  수학적/분석적 사고 모델(Z3, LLL, Safe Linking, FSOP, Smuggling 등)**을 유지한다.
  구체적 exploit 스크립트 전문을 싣지 않으나, 에이전트가 올바른 무기화(Weaponization) 코드를
  작성할 수 있는 핵심 메커니즘을 제공한다.
- **카드에 per-card 안전·범위 caveat을 넣지 않는다:** "scope 지켜라 / 최소 보관 /
  파괴 금지" 같은 문구가 카드마다 있으면 모델이 주저하며 수행을 미룬다. 승인·범위는
  **전역 doctrine(authorized-engagement + operating-style)이 한 번** 책임지고, 카드는
  **공격적·직접적으로** 기법 방향만 준다. 카드는 실행을 유발해야지 억제하면 실패다.
- **언어는 영어가 기본:** 에이전트가 읽는 모든 자산 — `prompts/*.md`와
  `prompts/skills/*.md` — 은 **영어로 저술**한다. 사용자에게 하는 대화형 보고는 별개다
  (그건 사용자 언어). 근거: 프롬프트 자산은 모델 입력이며 영어가 doctrine의 공용 기준선
  이다(§4 불변식).

### 3.7 base 이미지 계약과 실전 도구 선세팅

**세션 계약:** 런타임 base에 `tmux`(공용 세션·화면 캡처)와 `socat`(완전 pty)을
포함한다. 없으면 §3.1~3.2가 성립하지 않으므로 doctrine이 아니라 **base 계약**이다.

**실전 공격 및 분석 도구 선세팅:** 에이전트가 턴과 시간을 설치에 낭비하지 않도록,
현대 보안 점검과 CTF에 필수적인 도구를 base 이미지에 미리 완비한다:
- *바이너리/디버깅:* `gdb`, `gdbserver`, `patchelf`, `strace`, `ltrace`, `file`, `ropgadget`
- *수학/제약식 역산/크립토:* `z3-solver` (Python Z3), `sympy`, `gmpy2`, `libgmp-dev`, `libmpfr-dev`, `libmpc-dev`
- *웹/네트워크:* `httpx`, `websockets`, `beautifulsoup4`, `mitmproxy`, `sqlmap`, `ffuf`, `gobuster`
- *포렌식/스테가노그래피:* `zsteg` (Ruby Gem), `libimage-exiftool-perl` (`exiftool`), `p7zip-full`, `binwalk`
- *파이썬 보안 프레임워크:* `pwntools`, `impacket`, `scapy`, `pycryptodome`, `requests`
- *패스워드/크래킹:* `hashcat`, `john`, `medusa`, `ncrack`, `crunch`, `cewl` + `rockyou` 워드리스트

**나머지는 에이전트가 런타임에 자율 조달:** 추가 도구는 에이전트가 직접 설치한다.
이를 위해 런타임 사용자에게 **무암호 sudo**를 부여한다(`sudo apt`, 그리고
`pip3`/`gem`/`cargo`/정적 바이너리 다운로드). 큰 워드리스트(SecLists 등)는 워크스페이스로
직접 가져온다. **신뢰 경계는 격리된 컨테이너이지 이 in-container 사용자가 아니다**
(ADR-0001 §12) — 에이전트는 이미 이 컨테이너에서 완전한 셸을 가지므로, sudo는 새 경계를
넘는 게 아니라 조달·raw 소켓 능력을 더할 뿐이다. raw-socket 스캔·캡처(SYN 스캔·masscan·
tcpdump)를 위해 컨테이너를 `NET_RAW`/`NET_ADMIN` 권한으로 실행한다(compose에 반영).

**막히면 즉시 스크립트 작성 및 검색:** 도구·에러·기법에서 막히면 같은 실패를 반복하지 말고
파이썬 익스플로잇 스크립트를 직접 작성하거나, 브라우저·`curl`로 웹을 검색해 라이트업을
참조하여 즉시 우회 공략한다(doctrine `prompts/tradecraft.md`).

## 4. 불변식 (하지 않는 것)

- **터미널 에뮬레이터/PTY 데몬 코어 재구현 금지:** tmux/socat에 위임.
- **세션 IPC용 별도 데몬·소켓·RPC 금지:** 조종간 교대는 팀 버스, 화면은 직접 읽기.
- **코어 센티널/프롬프트 정규식 파싱 금지:** 출력 정착 판정은 모델 턴에서(마커 echo).
- **전역 세션 오염 금지:** run-id 네임스페이스 + 자동 정리.
- **최소 코어:** 코어 도구 표면 추가는 `shell.stdin` 하나. 세션 관리·상한·정리는
  런타임 계약, 방법론은 doctrine/디스크 자산.
- **운영 경계(ADR-0002 §3.1):** 리스너·발판 유지는 교전 scope 내 최소, 확인 후.
- **에이전트 자산 언어 = 영어(기본):** `prompts/*.md`와 `prompts/skills/*.md`는 영어로
  저술한다. 새 카드·doctrine도 영어. 사용자 대상 대화형 보고에는 적용하지 않는다.
- **카드는 공격적, per-card 안전문 금지:** 승인·범위는 전역 doctrine이 소유하고, 카드는
  기법 방향만 직접적으로 준다. 최신 공격 기법과 분석 프레임워크(Lattice, Modern Heap, Z3, Smuggling 등)를
  항상 강력하게 유지한다.

## 5. 대안과 기각

- **PTY 데몬 + UDS/RPC 제어평면(기각):** 수천 줄 코어·크로스플랫폼 부담. tmux가 이미
  검증된 형태로 제공. (`../pentesting`은 이 무거운 경로를 택했고, 본 저장소는 clean-room
  경량 경로를 택한다 — reference-only, 포팅하지 않음.)
- **세션 소유 worker + 형제가 RPC로 중계(기각):** 모든 I/O에 모델 턴이 끼어 느리고,
  4 KiB 경계에서 화면이 잘린다. 공용 책상 + 직접 읽기가 더 가볍고 자율적.
- **방법론을 프롬프트에 전부 굽기(기각):** a-z 지식을 매 턴 고정 비용으로 얹는다.
  디스크 라이브러리 + 선택적 읽기가 토큰 효율·자율성에서 우위.
- **개별 전용 공격 도구 표면 세분화(기각):** `nmap_scan`, `sqlmap_run`, `gdb_disasm`,
  `jwt_forge` 등 개별 공격 기능별로 LLM Function Tool을 수십 개 등록하는 방식을 기각하고,
  `shell`(stdin 포함), `workspace`, `team` 3대 범용 도구 체제를 확고히 유지한다.
  - **근거 1 (매 턴 수천 토큰 고정 낭비 방지):** 15~20개의 전용 도구 JSON Schema와 인자
    설명만으로 매 턴 3,000~6,000 토큰이 낭비되어 문맥 예산을 잠식한다. 3대 범용 도구는
    300 토큰 미만으로 끝나, 토큰을 순수 공격 증거와 기억(`brief.md`)에 온전히 할당할 수 있다.
  - **근거 2 (파이프라인 및 유기적 커맨드 조합성):** 실전 모의해킹/CTF는 정형화된 단일 API가
    아니라 `nmap ... | grep open | cut ... | xargs ffuf ...` 같은 파이프라이닝, 서브셸,
    리다이렉션, 즉석 파이썬 원라이너(`python3 -c "..."`)의 유기적 결합이 본질이다. 도구를 쪼개면
    파이프 1개당 턴 수가 폭발하고 표현력이 극도로 제한된다.
  - **근거 3 (LLM의 리눅스 CLI/Python 친화력 극대화):** 프론티어 LLM은 임의의 커스텀
    Tool Schema보다 수억 줄로 사전 학습된 표준 Linux Bash 및 Python 스크립트 작성에 압도적으로
    최적화되어 있어 스키마 오류를 내지 않는다.
  - **근거 4 (런타임 코드 디커플링 및 영구적 안정성):** 신규 공격 기법·도구 등장 시 Rust
    코드를 수정·재컴파일할 필요 없이 Docker 베이스 이미지(`apt`/`pip`) 패키징이나 런타임
    자율 조달(`sudo apt`)로 즉시 대응할 수 있다.
  - **결론 ("3-Layer Golden Rule"):** 도구 인터페이스(Rust 코어)는 극도로 최소화하고,
    실행 환경(Docker)은 풍부하게 사전 완비하며, 지식 베이스(Skills)는 온디스크로 분리하는
    3계층 분리가 최적의 아키텍처다.

## 6. 구현 범위와 상태

**이번 증분(완료):**
- `prompts/skills/` 방법론 라이브러리 2.0 (INDEX + 19종 전 카테고리 카드), clean-room, 최신 공격 프리미티브 완비.
- `prompts/tradecraft.md` doctrine 상시 포함(build_system 조립·README 반영) 및 커스텀 스크립트 무기화 지침 강화.
- `prompts/authorized-engagement.md`의 자율 헤드리스 실행 중 무인 중단(Hang) 방지 지침 보강.
- base에 `tmux`/`socat` 추가, app 이미지에 `prompts/skills/` → `/opt/minimal-agent/skills` 복사.
- base에 실전 분석 툴체인(gdb, patchelf, z3-solver, sympy, gmpy2, httpx, zsteg, exiftool 등) 선설치,
  런타임 사용자 무암호 sudo(자율 조달), compose에 `NET_RAW`/`NET_ADMIN`(raw-socket 스캔·캡처).

**다음 증분(미구현, 이 ADR이 규정):**
- 공용 세션 관리와 §3.4 안전 레일(단일 기록자·run-id 네임스페이스·자동 정리·상한)의
  런타임 구현과 Docker 게이트 검증.
- `shell.stdin` 프리미티브(§3.3).
- 증거: 리버스 셸 catch → pty 업그레이드 → 캡처를 실제로 수행하는 Docker E2E 스모크.
  이 로그가 나오기 전까지 본 ADR은 Proposed로 유지하고 crate 버전을 올리지 않는다.

## 7. 완료 정의

§3.4 안전 레일이 런타임에서 강제되고, §6의 E2E 스모크가 Docker 게이트에서 초록이며,
방법론 라이브러리가 이미지에 실려 에이전트가 자율적으로 참조함이 확인되면 Accepted로
승격하고 ADR-0002와 함께 0.111.0으로 릴리스한다.
