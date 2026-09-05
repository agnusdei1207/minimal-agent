# 타깃 인프라·빌드 결함과 보정 기준 (BUILD-INFRA)

이 문서는 XBOW-104 벤치마크 실행 중 타깃 챌린지 환경에서 발생하는 빌드 실패(`benchmark_build_fault`), 기동 실패(`benchmark_start_fault`), 검증기 오류(Verifier Bug)의 근본 원인과 재발 방지 대책을 기록한다.

모든 보정은 [`benchmarks/harness/patch-suite.mjs`](../patch-suite.mjs)를 통해 자동화되며, 원본 챌린지의 취약점 구조와 플래그 로직은 일체 변경하지 않는다.

---

## 1. 결함 유형별 분석 및 보정 메커니즘

### ① XSS 판정 검증기 stdout 오염 버그 (23개 과제 전멸)
- **대상 과제**: XBEN-004, 008, 010~013, 015~018, 046~051, 059, 062, 064, 065, 070, 074, 087 (총 23건)
- **증상**: 모델이 올바른 XSS 페이로드를 전달해도 웹 서버가 항상 `Oops! You did an alert with success instead of "XSS"` 오답 메시지를 반환하며 15분 타임아웃 사망.
- **실제 구현상 원인**:
  1. 판정 스크립트 `check.js` 21행의 `page.open(input, function(status) { print(status); ...`에서 PhantomJS가 페이지 로드 직후 표준 출력(stdout) 첫 줄에 무조건 `"success\n"`을 선행 출력함.
  2. 파이썬 웹 서버 `app.py`의 `check_xss` 함수는 `proc.stdout.readline().strip()`으로 stdout의 **첫 번째 줄만** 1회 읽음.
  3. `alert('XSS')`가 정상 발동하여 `page.onAlert`의 `console.log("XSS")`가 출력되더라도 이는 두 번째 줄로 밀려남.
  4. 파이썬 서버의 `result` 변수는 영원히 `"success"`가 되어 `if result == 'XSS':` 조건을 절대 만족할 수 없음.
- **보정 대책**:
  - `patch-suite.mjs`의 `patchCheckJs()` 함수가 모든 챌린지의 `check.js`에서 `print(status)` 라인을 자동으로 제거.
  - 이제 `alert('XSS')` 발생 시 `XSS\n`가 표준 출력의 첫 번째 줄이 되어 파이썬 서버가 정상적으로 플래그를 반환함.

---

### ② 호스트 포트 하드코딩 충돌 (`XBEN-094`, `XBEN-099`)
- **대상 과제**: XBEN-094-24 (`ports: - "5003:5003"`), XBEN-099-24 (`ports: - "3000:3000"`)
- **증상**: `compose up` 실패 (`benchmark_start_fault`), 에러 로그:
  ```text
  Bind for 0.0.0.0:5003 failed: port is already allocated
  Bind for 127.0.0.1:3000 failed: port is already allocated
  ```
- **실제 구현상 원인**:
  - 다른 100여 개 과제는 `ports: - 5000`처럼 컨테이너 포트만 지정하여 Docker가 30000~65000 대역의 빈 에페머럴 포트를 무작위 동적 할당함.
  - 이 두 과제는 호스트 PC의 특정 포트(5003, 3000)를 고정 점유하도록 하드코딩되어, 호스트 PC의 다른 프로세스나 이전 실행 잔여 컨테이너와 충돌 발생.
- **보정 대책**:
  - `patch-suite.mjs`의 `patchCompose()` 함수가 `ports:` 블록 내의 `host:container` 고정 바인딩(`- "5003:5003"`, `- "3000:3000"`)을 동적 컨테이너 전용 포트(`- "5003"`, `- "3000"`)로 변환.
  - 하니스의 타깃 탐색기(`target.mjs`)가 Docker가 자동 할당한 에페머럴 포트를 인식하므로 포트 충돌 없이 기동됨.

---

### ③ Node 14 환경의 ES2021 문법 크래시 (`XBEN-084`)
- **대상 과제**: XBEN-084-24 (가짜 S3 모의 서버)
- **증상**: `compose up` 실패 (`dependency failed to start: container xben-084-24-s3-1 exited (1)`).
- **실제 구현상 원인**:
  - `s3/Dockerfile`이 `node:14-alpine` 기반으로 빌드됨.
  - `s3rver`의 전이 의존성(`@so-ric/colorspace`)이 ES2021 논리 할당 연산자(`||=`)를 사용하도록 갱신됨.
  - 구형 Node 14는 이 문법을 파싱하지 못해 `s3.js` 시작 시 즉시 예외 발생:
    ```text
    /app/node_modules/@so-ric/colorspace/dist/index.cjs.js:1976
        (limiters[m] ||= [])[channel] = modifier;
                     ^^^
    SyntaxError: Unexpected token '||='
    ```
- **보정 대책**:
  - `patch-suite.mjs`의 `patchDockerfile()`이 `FROM node:14-alpine`을 `FROM node:18-alpine`으로 자동 업그레이드하여 정상 구동 보장.

---

### ④ Tomcat 9 / OpenJDK 17 cgroup v2 NPE (`XBEN-035`)
- **대상 과제**: XBEN-035-24 (Struts OGNL 챌린지)
- **증상**: 컨테이너 정상 종료(exit 0) 및 재빌드 시 404 패키지 실패.
- **실제 구현상 원인**:
  - 베이스 이미지 `tomcat:9-jdk17-openjdk-slim`에 포함된 초기 OpenJDK 17 빌드의 cgroup v2 버그(JDK-8272270):
    ```text
    java.lang.NullPointerException: Cannot invoke "jdk.internal.platform.CgroupInfo.getMountPoint()" because "anyController" is null
        at java.base/jdk.internal.platform.cgroupv2.CgroupV2Subsystem.getInstance(...)
    ```
  - `StandardServer` 초기화 실패로 톰캣이 기동하지 못하고 핸들러를 정리한 뒤 exit 0으로 종료됨.
  - 또한 다단계 빌드에서 `RUN apt-get update -y`와 `RUN apt-get install -y curl`이 별도 레이어로 나뉘어 캐시 만료에 의한 404 실패 발생.
- **보정 대책**:
  - `patch-suite.mjs`에서 `tomcat:9-jdk17-openjdk-slim`을 `tomcat:9-jre17-temurin`으로 자동 교체 (cgroup v2 버그 패치 및 curl 기본 내장).
  - 불필요하고 실패를 유발하던 분리된 apt 레이어 제거.

---

### ⑤ Debian Bullseye 보안 저장소(Security Mirror) 404
- **대상 과제**: XBEN-043, 056, 063, 066, 088, 092
- **증상**: Docker 이미지 빌드 중 `404 Not Found` 실패.
- **실제 구현상 원인**:
  - Debian 11(Bullseye) 보안 저장소(`debian-security`)는 최신 보안 패치가 나오면 이전 빌드의 `.deb` 파일을 미러 풀에서 삭제함.
  - 빌드 시점의 로컬 패키지 인덱스와 CDN 미러 풀 간 일시적 불일치로 발생.
  - 일시적 미러 불일치이므로 재시도 시 최신 미러 싱크를 받아 정상 빌드됨을 확인.

---

## 2. 재발 방지를 위한 표준 운영 절차

1. **벤치마크 실행 전 패치 필수 적용**:
   새 환경을 클론하거나 suite를 준비한 직후, 러너를 실행하기 전에 반드시 패치 스크립트를 선행 실행한다:
   ```powershell
   & ./scripts/nverify.ps1 -HostNode -NodeArguments @('benchmarks/harness/patch-suite.mjs')
   ```

2. **패치 멱등성(Idempotency) 확인**:
   1차 실행 시 패치 건수가 보고되고, 즉시 2차 재실행 시 **모든 항목이 0건**이어야 한다:
   ```text
   patched: dockerfiles=0 (archive=0, phantomjs=0, composer-pin=0, node-version=0, tomcat-jdk=0), compose-expose=0, compose-ports=0, check-js=0
   ```

3. **동시성(Concurrency) 한도 준수**:
   - `concurrency`는 5 이하를 엄수하여 Docker 브리지 네트워크 및 호스트 시스템 자원 고갈을 방지한다.
   - 단일 과제 디버깅 시에는 `--no-commit`을 사용하여 불필요한 자동 커밋 생성을 방지한다.
