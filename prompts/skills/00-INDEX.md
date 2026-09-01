# Attack Methodology Library (autonomous reference)

This folder holds **principle-level direction cards** for international CTF and advanced authorized offensive operations. Each card provides mental models, attack arcs, key primitives, and signals to identify targets without rigid scripts.

## Usage (autonomous)

1. When you face a target class, **skim this index and read the one or two relevant cards.**
2. A card is analytical direction, not a copy-paste recipe. Adapt it; build targeted exploit/solver scripts (pwntools, Z3, Scapy, curl, Python) directly.
3. If several domains overlap (e.g. Linux kernel behind a web app), combine their mental models.
4. If a technique fails after two iterations, record the dead end and pivot to an alternative hypothesis.

## Cards

| # | Card | Primary Domain / Triggers |
|---|---|---|
| 01 | [Recon & Enumeration](01-recon-and-enumeration.md) | Surface mapping, port/service scans, vhost/endpoint/DNS discovery |
| 02 | [Web Application](02-web-application.md) | Injection (SQL/SSTI/Cmd), Prototype Pollution, Deserialization, SSRF, Auth |
| 03 | [Network Sniffing & MITM](03-network-sniffing-and-mitm.md) | Cleartext protocols, ARP/LLMNR/mDNS spoofing, traffic interception & relay |
| 04 | [Password & Credential Attacks](04-password-and-credential-attacks.md) | Hash cracking (hashcat/john), Kerberoast/AS-REP, spraying, token reuse |
| 05 | [Linux Privilege Escalation](05-linux-privilege-escalation.md) | SUID/capabilities, sudo misconfigs, cron/service hijacking, kernel & container breakout |
| 06 | [Windows & Active Directory](06-windows-and-active-directory.md) | AD trust graph, Kerberos abuse, ADCS (ESC1-8), RBCD, token impersonation |
| 07 | [Reverse Shells & Post-Exploitation](07-reverse-shells-and-post-exploitation.md) | Shared-session listeners, PTY upgrade, egress tunneling (chisel/ligolo), pivoting |
| 08 | [Binary Exploitation (pwn)](08-binary-exploitation-pwn.md) | Stack ROP/SROP, Heap (tcache/Safe Linking/FSOP House of Apple), Kernel & JIT |
| 09 | [Reverse Engineering](09-reverse-engineering.md) | Static/dynamic analysis, Z3 SMT solver, angr symbolic exec, VM deobfuscation |
| 10 | [Cryptography Attacks](10-cryptography-attacks.md) | Lattice (LLL/Coppersmith/CVP), ECDSA nonce bias, RSA variants, padding oracles |
| 11 | [Embedded & Firmware](11-embedded-and-firmware.md) | Firmware unpacking (binwalk/ubifs), hardcoded secrets, QEMU emulation, UART/JTAG |
| 12 | [Robotics / ICS / OT](12-robotics-ics-ot.md) | Modbus/S7/CIP/DNP3, PLC register manipulation, unauthenticated ROS1/2 topics |
| 13 | [Cloud & Containers](13-cloud-and-containers.md) | AWS/GCP/Azure IMDS & IAM policies, container escape (cgroups/sockets), K8s RBAC |
| 14 | [Wireless & RF](14-wireless-and-rf.md) | 802.11 WPA2/Enterprise handshake/PMKID, BLE GATT abuse, SDR signal analysis |
| 15 | [HTTP Intercept & Replay](15-http-intercept-and-replay.md) | HTTP Request Smuggling (CL.TE/TE.CL/H2), single-packet race conditions, WebSockets |
| 16 | [Forensics](16-forensics.md) | Volatility 3 memory analysis, PCAP stream/USB HID reconstruction, filesystem/EVTX |
| 17 | [Steganography](17-steganography.md) | Bit-plane/LSB (zsteg), DCT frequency domain, appended data, audio spectrograms |
| 18 | [Smart Contracts & Web3](18-smart-contracts-web3.md) | Reentrancy, flash loan oracle manipulation, storage collision, delegatecall abuse |
| 19 | [AI & LLM Security](19-ai-and-llm-security.md) | Prompt injection (direct/indirect), model deserialization RCE, adversarial inputs |
