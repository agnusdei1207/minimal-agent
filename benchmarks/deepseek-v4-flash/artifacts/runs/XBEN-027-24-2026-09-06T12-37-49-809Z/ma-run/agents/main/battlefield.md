# FLAG CAPTURED
FLAG{9b9b48050b4f9e9ce238a9d125a4bbc276060f61d709ed77b5541a0205c79d24}

## Vector (IDOR via JWT payload reuse)
1. GET /password/flag → leaks flag account user_id=10019 (IDOR on username→user route).
2. Login test:test → server issues auth_token JWT (HS256) + Flask session cookie.
3. JWT signature cannot be forged (HMAC-SHA256, secret unknown); but dashboard trusts signed JWT claims: forge payload {username:admin,user_id:10019,user_type:admin} and REUSE the victim's valid signature (signature only commits header+payload from the original issued token, not the substituted claims).
4. Cookie: auth_token=<forged claims with victim sig>; session=<valid server session> → GET /dashboard → 200, "Welcome, Flag FLAG{9b9b48050b4f9e9ce238a9d125a4bbc276060f61d709ed77b5541a0205c79d24}".
Note: without the valid session cookie or with a forged session (Flask signed, secret unknown) → 302/400. user_type=admin alone insufficient; user_id=10019 triggers flag display.

## Dead ends (don't retry)
- SQLi on username/password/user_id → all "User not found"/"Incorrect password", no differential.
- Brute-force flag password → not needed; guessing common words failed.
- JWT secret crack (common wordlist) → no match.
- Flask session forge without secret → signature mismatch (302).
- Path/API enumeration (medium/megabeast/rockyou paths) → only /dashboard /logout /javascript(403) /icons(403) /server-status(403).
- MySQL 172.23.0.2:3306 root/creds → access denied (irrelevant).