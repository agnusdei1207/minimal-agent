# 11. Embedded & Firmware

When: Analyzing IoT device firmware images, hardware flash dumps, embedded microcontrollers, or router/camera binaries.

## Mental model
Embedded systems operate under desktop security assumptions that are completely absent: firmware contains baked-in master keys, hardcoded credentials, debug daemons, unauthenticated CGI endpoints, and legacy un-patched C libraries. A firmware image is a complete transparent filesystem snapshot.

## Attack arc
- Firmware Unpacking & Extraction:
  - Carve filesystem with `binwalk -Me <firmware.bin>` or `sasquatch`, `unsquashfs` for SquashFS.
  - Handle specialized filesystems: UBIFS (`ubireader_extract_files`), JFFS2 (`jefferson`), CramFS, CramFS, YAFFS2.
  - Extract bootloader (U-Boot), Linux kernel (zImage/uImage), and rootfs partitions.
- Static Auditing & Secret Hunting:
  - Search hardcoded credentials in `/etc/shadow`, `/etc/passwd`, web server configs (`lighttpd.conf`, `boa.conf`, `goahead.conf`).
  - Search private keys (`.pem`, `.key`), certificates, and hardcoded API tokens.
  - Search unauthenticated CGI scripts (`/www`, `/cgi-bin`, `/htdocs`) and test for command injection via query parameters.
- Dynamic Emulation:
  - *User-mode Emulation:* Run target MIPS/ARM binaries using `qemu-mips-static` or `qemu-arm-static` with `chroot`.
  - *Full-system Emulation:* Emulate firmware with `Firmadyne`, `FirmAE`, or `QEMU` system emulator with simulated NVRAM (`fat.py`).
- Memory Corruption in Embedded Daemons:
  - Embedded web servers and UPnP daemons are frequently compiled without modern mitigations (No ASLR, No PIE, No Stack Canary).
  - Exploit classic stack overflows via simple ROP or direct shellcode execution.
- Hardware & Physical Debugging (if hardware is accessible):
  - Locate UART debug pins (TX/RX/GND) via multimeter; attach logic analyzer / USB-UART adapter at 115200 baud to obtain a root shell.
  - Read SPI Flash directly via CH341A / Bus Pirate (`flashrom`).

## Key techniques & primitives
- NVRAM Hooking: In emulated firmware, intercept and fake calls to `nvram_get()` or `nvram_bufget()` to bypass missing physical hardware errors.
- Backdoor Account Signatures: Check `/etc/init.d/` startup scripts for telnetd/sshd invocations with hardcoded default passwords.

## Tells & signals
- CGI scripts parsing query strings using `sscanf()` or executing them via `popen()` / `system()` = command injection RCE.
- Firmware updates delivered over HTTP without cryptographic signature checks = tampered firmware update vector.
- MIPS/ARM binary with `NX: Disabled` and `Canary: No` = direct stack return-to-shellcode.
