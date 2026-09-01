# syntax=docker/dockerfile:1.12

FROM ubuntu:26.04

ARG CHROME_FOR_TESTING_VERSION=152.0.7977.64
ARG CHROME_FOR_TESTING_SHA256=8b592f066af71f054aab2cc80fc26f73c775c6d44ebb99d16ade924b24756c2e

ENV DEBIAN_FRONTEND=noninteractive \
    TERM=xterm-256color \
    COLORTERM=truecolor \
    FORCE_COLOR=3 \
    LANG=en_US.UTF-8 \
    LC_ALL=en_US.UTF-8

RUN if [ -f /etc/apt/sources.list.d/ubuntu.sources ]; then \
      sed -i 's/Components: main/Components: main universe multiverse/g' /etc/apt/sources.list.d/ubuntu.sources; \
    fi \
    && apt update -qq \
    && apt install -y --no-install-recommends \
      bash \
      ca-certificates \
      curl \
      dirb \
      dnsutils \
      ffuf \
      findutils \
      fonts-liberation \
      fzf \
      git \
      gobuster \
      hydra \
      iproute2 \
      iputils-ping \
      jq \
      libasound2t64 \
      libatk-bridge2.0-0t64 \
      libatk1.0-0t64 \
      libcairo2 \
      libcups2t64 \
      libdbus-1-3 \
      libdrm2 \
      libexpat1 \
      libgbm1 \
      libglib2.0-0t64 \
      libnspr4 \
      libnss3 \
      libpango-1.0-0 \
      libx11-6 \
      libxcb1 \
      libxcomposite1 \
      libxdamage1 \
      libxext6 \
      libxfixes3 \
      libxkbcommon0 \
      libxrandr2 \
      locales \
      masscan \
      ncurses-term \
      net-tools \
      netcat-traditional \
      nikto \
      nmap \
      openssh-client \
      procps \
      python3 \
      python3-pip \
      ruby-full \
      smbclient \
      socat \
      sqlmap \
      sudo \
      tcpdump \
      tmux \
      unzip \
      util-linux \
      whatweb \
    && sed -i '/en_US.UTF-8/s/^# //g' /etc/locale.gen \
    && locale-gen \
    && rm -rf /var/lib/apt/lists/*

# Offensive-security tooling, famous wordlists, and Python libraries preinstalled so
# the agent has the common basics out of the box (ADR-0003 §3.7). Beyond these, agents
# install what they still need at runtime (they have passwordless sudo). A rockyou
# fetch that flakes must not brick the base image, so it is best-effort — the agent can
# always pull wordlists (e.g. SecLists) itself when needed.
RUN apt update -qq \
    && apt install -y --no-install-recommends \
      build-essential \
      cmake \
      pkg-config \
      python3-dev \
      libffi-dev \
      libssl-dev \
      libgmp-dev \
      libmpfr-dev \
      libmpc-dev \
      gdb \
      gdbserver \
      file \
      patchelf \
      strace \
      ltrace \
      p7zip-full \
      libimage-exiftool-perl \
      hashcat \
      john \
      medusa \
      ncrack \
      crunch \
      cewl \
      wfuzz \
      dnsrecon \
      dnsenum \
      smbmap \
      nbtscan \
      onesixtyone \
      snmp \
      ldap-utils \
      binwalk \
      netcat-openbsd \
      sshpass \
      proxychains4 \
      wget \
    && rm -rf /var/lib/apt/lists/* \
    && gem install --no-document zsteg \
    && pip3 install --no-cache-dir --break-system-packages \
      pwntools \
      ropgadget \
      z3-solver \
      sympy \
      gmpy2 \
      impacket \
      scapy \
      pycryptodome \
      requests \
      httpx \
      websockets \
      beautifulsoup4 \
      dnspython \
      mitmproxy \
    && mkdir -p /usr/share/wordlists \
    && (curl -fsSL -o /usr/share/wordlists/rockyou.txt \
         https://github.com/brannondorsey/naive-hashcat/releases/download/data/rockyou.txt \
         || echo "[warn] rockyou fetch failed at build; agent fetches wordlists at runtime")

COPY docker/install-browser.sh /tmp/install-browser.sh
RUN bash /tmp/install-browser.sh "${CHROME_FOR_TESTING_VERSION}" "${CHROME_FOR_TESTING_SHA256}" \
    && rm /tmp/install-browser.sh \
    && groupadd --gid 10001 minimal-agent \
    && useradd --uid 10001 --gid 10001 --create-home minimal-agent \
    && mkdir --parents /workspace /state \
    && chown --recursive minimal-agent:minimal-agent /workspace /state \
    && echo 'minimal-agent ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/minimal-agent \
    && chmod 0440 /etc/sudoers.d/minimal-agent

WORKDIR /workspace
