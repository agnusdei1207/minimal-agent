# Main Brief — CTF web:80 — SOLVED
## Flag
FLAG{c1aa7b7022dd4a9c8e78927cfd19d88d18a26cd73d4cca0d153f6a2e8d5170b5}
## Winning path
1. search.php POST username = double-quote SQLi: `"/**/or/**/1#` → TRUE.
2. Blocklist (case-INSENSITIVE substrings): space/tab/newline, and, where, limit, like. Not blocked: union, select, from, group_concat, separator, or, comma, quotes, #.
3. UNION SELECT 3 cols (col2 echoed). Columns guessed (info_schema unusable: 'where' unbreakable keyword).
4. users(id, username, password): `"/**/union/**/select/**/1,group_concat(username,0x3a,password),3/**/from/**/users#` → admin:ThisIsBestPassword.
5. login.php admin/ThisIsBestPassword → "Congratz, flag is FLAG{...}".
## Lessons
- 'Blocked' ≠ 500; blocklist probed char-by-char.
- Double-quote context (single-quote probes were inert).
- /**/ replaces spaces (real bypass, comments permitted).
- Case-insensitive blocks can't be bypassed by comment-split keywords (breaks SQL); route around (WHERE-free queries, column guessing).
## Dead ends
- '||' concat, boolean 'or' tautologies with single quotes, information_schema WHERE queries, magic login strings, hidden files/paths (only 403s), getcwd error leak (display_errors off), counting rows.