# 16. Forensics

When: Analyzing memory dumps, disk/filesystem images, packet captures (PCAP), corrupted files, or event logs in CTF challenges and incident investigations.

## Mental model
Digital forensics is reconstructing ephemeral execution state from residual storage artifacts: finding data in deleted blocks, unallocated space, memory structures, network streams, and logging trails that the system attempted to discard or conceal.

## Attack arc
- Memory Forensics (Volatility 3):
  - Run `vol -f <dump.raw> windows.info` or `linux.banner` to identify OS profile.
  - *Process & Injection Triage:* `windows.pslist`, `windows.pstree`, `windows.psscan` (unlinked processes). Inspect injected code/hollowed processes via `windows.malfind`.
  - *Extract Secrets & Files:* `windows.hashdump`, `windows.lsass`, `windows.dumpfiles --pid <PID>`, `windows.cmdline`.
  - *Network Artifacts:* `windows.netscan` (active and closed TCP/UDP connections).
- Network PCAP Deep Inspection:
  - Analyze streams with `tshark` / `wireshark` display filters (`http`, `dns`, `smb2`, `frame contains "flag"`).
  - Reassemble TCP streams and export transferred files (`tshark -r capture.pcap --export-objects "http,dest_dir"`).
  - Decrypt TLS streams using supplied SSL Keylog files (`wireshark -o tls.keylog_file:keys.log`).
  - *Specialized CTF Protocols:* Parse USB HID captures (reconstruct mouse movement trajectories or keyboard HID scan codes to extract typed passwords/flags using Python).
- Disk & Filesystem Artifacts:
  - *Deleted File Recovery:* Use The Sleuth Kit (`fls -r -p image.dd`, `icat`), `foremost`, or `scalpel` to carve unallocated space.
  - *NTFS Deep Analysis:* Parse Master File Table (`$MFT` via `MFTECmd`), `$LogFile`, and `$UsnJrnl` for file modification timelines.
  - *Windows Execution Artifacts:* Prefetch (`PECmd`), Shimcache / Amcache (`AmcacheParser`), Jump Lists, Registry hives (`regrip`).
  - *Event Logs:* Parse EVTX logs (`evtx_dump`) for Event IDs 4624 (Logon), 4688 (Process Creation), 4104 (PowerShell ScriptBlock Execution).
- Corrupted / Truncated File Carving:
  - Fix corrupted magic bytes and headers (PNG IHDR chunks, ZIP central directory records, PDF cross-reference tables) via hex editor (`hexedit`, `xxd`).

## Key techniques & primitives
- USB HID Keystroke Reassembly:
  Parse byte 2 of 8-byte USB HID interrupt packets: map scan code hex values (`0x04`='a', `0x1e`='1') to ASCII characters, accounting for shift modifier byte (`0x02` / `0x20`).
- Volatility 3 Malfind Extraction:
  `vol -f dump.raw windows.malfind --pid <PID> --dump # dumps PAGE_EXECUTE_READWRITE injected memory segments`.

## Tells & signals
- `PAGE_EXECUTE_READWRITE` (VAD permissions) with unbacked memory = process injection / Cobalt Strike beacon in RAM.
- File magic bytes mismatch (e.g. file starts with `PK\x03\x04` but named `.png` = ZIP archive).
- USB traffic with 8-byte endpoint data packets = HID keyboard/mouse capture.
