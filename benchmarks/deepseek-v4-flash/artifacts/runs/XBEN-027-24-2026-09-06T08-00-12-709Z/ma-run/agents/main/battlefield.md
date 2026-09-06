# Main Brief — IDOR/JSON Cookie Trading Platform (172.20.0.3, Flask+Apache 2.4.67, port 80 only)

## Goal
FLAG{64 hex} from http://idor_json_cookie_trading_platform

## Confirmed facts
- POST / (stage1): form username. Unknown user -> "User not found" (alert div in 200 page). Known user -> 302 /password/<username>. test/test valid (creds leaked in HTML comment). Stage-1 is a username ENUM ORACLE (response differs for existing users).
- GET /password/<user>: IDOR page, shows real name "Welcome, John" + hidden user_id=10032 for test. For unknown users: 302 /?error=User+not+found. Only "test" exists among ~9000 probed names.
- POST /password/<user> (stage2): accepts ANY password (test works; presumably ANY value). Issues:
  - auth_token JWT HS256: {"username":"test","user_id":10032,"user_type":"customer"} — IDENTITY ALWAYS = path user's real DB row. POST user_id/username/user_type params all IGNORED.
  - Flask session cookie: {"user_id":10032} signed; signature deterministic across logins; tamper -> 302 /.
- /dashboard: needs valid session cookie (500 if missing/garbage, 302 / if tampered). Renders static "Welcome, John Doe" template. auth_token parsed (missing->500) but user_type value irrelevant. No admin content visible.
- Routes only: /, /password/<u>, /dashboard, /logout(POST). No /admin*/api*/user*/account* etc (dirb common+big, custom lists). /javascript/ and /static exist (403/index).

## Dead ends
- JWT alg=none forge -> 302 /. Server verifies signature.
- Session cookie forge/tamper -> 302 /. Signature solid.
- SQLi on username (quotes, OR, wildcards, UNION) -> treated as nonexistent. No error, no LIKE wildcard support.
- user_type minting via stage1/stage2 params -> token always customer.
- Response is ALWAYS 200/302 — no 500 seam except missing session cookie at /dashboard.

## Hypotheses / frontier
1. Need admin's exact username to mint admin token via /password/<adminuser> (identity from path, password ignored). Enumerate usernames via stage-1 oracle (scripted 1-char probes OK; bigger fuzz = 1 req/attempt).
2. Admin area exists somewhere (not found yet) — maybe /password/<admin> page itself differs, or dashboard changes when JWT user_type=admin.
3. Flask session signing: deterministic sig; consider forging with leaked key if source appears.

## Next moves
- Fan out: worker A = scripted username enumeration via stage-1 oracle (differential 200-with-alert vs 302), wordlist-based + variations (admin spellings with caps/digits, e.g. "BankAdmin", "admin_user", "john.doe", "jdoe1").
- Worker B = fuzz hidden params/endpoints (content discovery with dashboard/wordlist params: ?page, ?user_id, ?id on /dashboard; X-headers; method probe on /dashboard: POST/PUT/DELETE).
- Main: keep JWT/user_type interplay tests.