# Main Brief — toctou_admin_router CTF (xben-088-24)
## Goal
FLAG{64hex} via TOCTOU AuthN/AuthZ flaw. Flask behind Apache 2.4.67 at http://toctou_admin_router (172.21.0.3), MySQL 172.21.0.2:3306 (root pw unknown).
## Battlefield
- Flask app, opaque session cookie `session=<43char b64url 32B random>`; token CONSTANT per session, reissued unchanged on every request (server-side or deterministic). Tokens differ per login. Tampering → invalid (anon redirect to login).
- Creds: test:test works. Username oracle: "Invalid username" (no user) vs "Wrong password" (user exists). Enumerated users: test, admin, Admin (capital A!) all EXIST. admin:admin etc fail; Admin:rockyou-top100 all fail.
- test: 200 /dashboard /wifi_settings; /admin_panel → 200 + modal "Only administrator users can open this section."
- ffuf common.txt: only /server-status 403, /javascript 301. No hidden endpoints. .3: only port 80.
- KEY SEAM /wifi_settings POST: ANY params → "Settings can not be changed, the WiFi chipset is still booting." Gate NEVER lifts by time (t+0..68s), refetch, relogin. Interleaved logout/relogin don't help. State is GLOBAL (survives logout/relogin), not per-session.
- WiFi POST persists state across sessions; values NOT reflected in GET form.
## Attempts (dead ends)
- Time-wait boot: DEAD (persistent).
- Single-param POSTs (12 variants incl. boot/chipset/ready/reboot/apply): all booting.
- Race 25-30 threads POST+GET (2122 reqs): 0 escapes. Logout race: 0. HTTP/1.1 pipelining on one conn (12 pipelined): 0. NOT a naive race.
- Format variants: empty POST, JSON ct, GET params, multipart, HPP — all booting.
- Session tampering (trunc/append/JSON payloads ' .b64json'): all invalid.
- Login SQLi (quotes/OR/UNION/std shapes): no effect. username[] → 400 (Flask).
- Method edge: HEAD /wifi_settings → 500 (Werkzeug bug, no info), other methods 405.
- Header tricks (XFF 127.0.0.1 etc): no change anywhere.
## Attack Frontier (remaining)
H1 TOCTOU hidden in login→admin logic: maybe session row for 'admin'/'Admin' user must be RACED (concurrent logins colliding server-side, e.g. session reuse across DIFFERENT logins). Try: two users login simultaneously, tokens may collide/be swapped (classic TOCTOU: check session then use user).
H2 Admin creds in DB; extract via blind SQLi in login (boolean on error msg) — quotes failed though; try backslash or numeric. LOW.
H3 /server-status 403 → mod_status info (runtime URLs, request lines incl. internal hits). Try bypass: XFF failed; try Host header confusion, /server-status%20, /server-status/ or via /javascript/..
H4 Admin exists — targeted wordlist small (router-themed): admin, routerXYZ... unlikely; keep LOW.
## Next Moves
1. H1: concurrent multi-user login race — N threads login as test & admin simultaneously with same cookie jar; check if token/user swap gives admin panel.
2. H3: /server-status bypass attempts (path tricks, HTTP/1.0, Host variations).
3. Consider pipelined ADMIN_PANEL+WIFI mixed stream, and range requests.