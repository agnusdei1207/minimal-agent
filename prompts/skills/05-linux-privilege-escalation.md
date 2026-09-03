# 05. Linux Privilege Escalation

When: You hold a standard user/service Linux shell and need root or elevated capabilities.

## Mental model
Privilege escalation is abusing delegated trust and misconfigured permissions: identifying an execution path where a lower-privileged context controls the inputs, environment, or file descriptors of a higher-privileged process.

## Attack arc
- Situational Awareness & Enumeration:
  - Check user id, groups (`id`), allowed sudo rules (`sudo -l`), Linux kernel version (`uname -a`).
  - Search SUID/SGID binaries: `find / -perm -4000 -type f 2>/dev/null`.
  - Check file capabilities: `getcap -r / 2>/dev/null`.
  - Inspect cron jobs, systemd timers, active network sockets (`ss -tulpn`), running processes (`ps aux`).
- Abuse Sudo & Execution Rights:
  - Exploit sudo binary privileges via GTFOBins (e.g. `vim`, `find`, `less`, `awk`, `tar`).
  - Check for environment inheritance: `SETENV`, `LD_PRELOAD`, `LD_LIBRARY_PATH`.
  - Wildcard injection in scripts invoked by root (e.g. `tar *` with `--checkpoint` filenames).
- Service & File Misconfigurations:
  - Writable scripts or binary directories in cron jobs or systemd services.
  - Writable sensitive files: `/etc/passwd` (inject root hash), `/etc/sudoers.d/`, `/etc/shadow`.
  - Shared library hijacking via `RPATH` / `RUNPATH` or missing shared objects.
- Kernel & OS Exploitation:
  - Match kernel release against known reliable exploits (Dirty Pipe CVE-2022-0847, Dirty COW, OverlayFS CVE-2023-2640, eBPF bugs).
- Container & Namespace Escape:
  - Mounted Docker socket (`/var/run/docker.sock`): run a container mounting host `/`.
  - Privileged containers (`--privileged`): cgroups `release_agent` execution or mount host block devices.
  - Dangerous capabilities: `CAP_SYS_ADMIN`, `CAP_SYS_PTRACE`, `CAP_DAC_READ_SEARCH`.

## Key techniques & primitives
- GTFOBins Patterns: SUID or sudo execution of binaries with shell escape functions, file write capabilities, or custom loadable plugins.
- Cgroups Escape: In a privileged container, create a cgroup, enable `notify_on_release`, and write a host-executing command path into `release_agent`.
- eBPF Exploitation: Unprivileged eBPF allows kernel memory read/write via verifier bugs or pointer arithmetic.

## Tells & signals
- Custom SUID binaries in `/opt`, `/tmp`, `/usr/local/bin` (reverse-engineer them for buffer overflows or unsafe `system()` calls).
- `sudo -l` showing wildcard commands `NOPASSWD: ALL` or script execution with writable paths.
- Processes running as UID 0 listening on `127.0.0.1`.
