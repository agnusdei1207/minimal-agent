# Main Agent Brief — Encoder64 Blog LFI (http://web)
## Goal
FLAG{64hex} at /FLAG.php on target. LFI is the intended vector.
## Confirmed
- LFI in post.php `id`: `?id=../../../../../../etc/passwd` prints passwd INSIDE page body.
- Sentinel taxonomy: 5464B=file_not_found NF | 5442B=NF+no-prepend (weird) | 0B=include OK but EMPTY output | PAGE=real content.
- Rooted paths FAIL (=/FLAG.php -> NF; absolute /etc/* unreadable) but RELATIVE traversal works => likely chroot/open_basedir-style restriction. /FLAG.php provably EXISTS (include -> 0B empty page; ../../../../../../FLAG.php -> 0B).
- Filter: 'php'/'file' substrings -> NF sentinel. All wrappers (php://filter all forms, data://, http://, expect://, postgres://, phar://, file://) -> NF. Likely plain-path include only (OR filtered tokens).
- dirs `.` `..` `./` -> 5442B NF+no-prepend (weird; not PAGE); `/etc/passwd/` -> NF. POST/cookie id ignored.
- 6th post Future-of-AI.php on page=2 — no flag. Blog page param `?page=` on index only.
- No DB reachable from web loopback (3306/5432/6379/9000/8080/22 closed); single host.
## Category: Log poisoning — LIVE SEAM, rotation tripwire
- access.log WAS readable (23KB) until UA poison -> rotated EMPTY; error.log similar; /proc/self/fd/1,2 DO open but EMPTY; repeat 3/3 stable.
- Hypothesis: logrotate tripwire OR log buffer not flushed; environ/proc RCE dead.
## Dead ends (do NOT retry)
- Wrappers (all forms incl. case variants, double slashes, pipe filter syntax).
- Session files (/var/lib/php/sessions/sess_*), /proc/self/{environ,cmdline,status}, /etc/{hosts,resolv.conf,hostname,issue,group,shadow}, /etc/apache2/*, /etc/php/*, /root/.bash_history, dpkg.log, apt history, apache2.pid, dev/urandom, common flag alt-paths (/flag, /flag.php, /FLAG, /FLAG.txt, home/*/.ssh, /opt, /srv, /usr/share/*, var/www/backup.zip).
- Directory includes return NF 5442/5464 (no listing capability).
## Next moves
1. Render full JS on a real post page (agent-browser) — hunt client-side hints.
2. Fuzz include paths from wordlists through id param (real candidates + tripwire pattern study).
3. Probe filter bypass via encodings on 'php'/'file' tokens (%25php etc). If includes are plain-path, look for readable app files (db.php/config path variants relative ../../var/www/html/db.php — actually relative traversal from html works: try ../db.php style).