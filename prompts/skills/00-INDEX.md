# Attack Methodology Library (autonomous reference)

This folder holds principle-level direction cards for international CTF and advanced authorized offensive operations. Each card provides mental models, attack arcs, key primitives, and signals to identify targets without rigid scripts.

## Usage (autonomous)

1. **Architecture Mental Model**: Do not attack blindly. Infer the technology stack, backend runtime, parsers, and companion services on the network.
2. **Attack Frontier (Breadth-First)**: Establish 3–5 structurally distinct hypotheses (auth/IDOR, injection, SSRF, state/logic, config) in your brief before diving deep.
3. **Signal Reading (Live Seam vs. Silent Wall)**:
   - *Silent Wall:* 3–5 probes with zero differential response (unchanged status/length) means the vector is unhandled. Mark as DEAD END and backtrack to another seam.
   - *Live Seam:* Error 500, syntax crash, reflection, timing delay, or "Blocked: X" filter response proves your input reached the backend interpreter! Do NOT abandon; apply creative bypasses.
4. **Creative Lateral Bypasses (Orthogonal Dimensions)**:
   - *Context:* Minimal break out of enclosing quotes, attributes, templates, or subshells.
   - *Encodings:* URL, double-URL, Unicode `\u`, Hex, Base64, parameter pollution.
   - *Delimiters:* Whitespace alternatives (`$IFS`, `/**/`, `%09`, `%0a`, `+`).
   - *Functional Equivalents:* If primary keywords (`alert`, `script`, `union`, `cat`) are blacklisted, use functional equivalents (`prompt`, `confirm`, `String.fromCharCode`, `tac`, `sh`, oracles).
   - *Client/Bot Compatibility:* For client-side exploitation against headless bot verifiers, always use backward-compatible ES5 JavaScript (`"..."`, regex `.source`, `String.fromCharCode`) to prevent silent crashes in legacy engines (PhantomJS, QtWebKit).
5. **Ban Low-Probability Sinks**: Never run dictionary cracking (rockyou) against unknown random secrets; never divert to steganography on UI decoration images (logos, icons); keep tooling remote and lightweight.

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
