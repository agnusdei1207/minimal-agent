# Main Brief — Encoder64 Blog LFI CTF
Goal: read /FLAG.php via protected LFI (http://web/post.php?id=). FLAG{64hex}.

## VERIFIED FACTS
- Oracle: byte-exact page per included file. Allowed: ScriptKiddie(8137B) EternalBlue(7484B) Intro-ML(7769B) Importance-Cybersec(7877B) Advances-Renewable(7875B). Everything else = generic 5464B "Post not found" (md5 d000bef93b2f5dc6261141b33c7bce49).
- Filter boundary (worker-03): EXACT regex-like ^\.\./ (exact case) + exact basename "ScriptKiddie.php" (etc). .././X and ..//X PASS (only ./ and // tolerated). ALL else fails: posts/ScriptKiddie.php, ../posts/../ScriptKiddie.php, suffix/space/null/case tricks, wrappers php://, data:, arrays, POST, cookie, HPP, %00, :$DATA.
- Apache: /posts/ 403(dir); /posts/<Post>.php served 200 EMPTY directly. /FLAG.php (webroot) genuine 404. /.htaccess 403 (exists). /server-status 403.
- Infra reads via LFI: all dead (../etc/*, ../proc/*, ../var/log/*, .htaccess).
- index.php decoy b64: e0knbV9hX1NjcmlwdF9LaWRkaWV9 = {I'm_a_Script_Kiddie}.
- /posts/ exists, /include/ exists (403s). No other dirs (gobuster common.txt clean).

## WORKING HYPOTHESES
H1 (lead): LFI allowlist checks CONTIGUOUS substring of one allowed filename (.././FLAG.php + prefix3 DID trigger 403). Bypass = path ending with allowed name that resolves elsewhere (symlink/jail), or feed allowed name with different cwd.
H2: hidden file in posts/ or include/ (workers fuzzing).
H3: Apache trick: filter is NOT keyed on FLAG name → posts/FLAG.php-style path with allowed-prefix returns 403 = live seam.

## OPEN THREADS
- worker-01: php://filter encodings (silent).
- worker-02: ffuf fuzz id= (silent).
- worker-03: infra (done/dead) + assigned Apache path-bypass matrix on /FLAG.php etc (pending).
- main: id=../EternalBlue.php does NOT leak index decoy (posts/ has own index).

## NEXT
1. worker-03 Apache matrix → if 403 seam confirmed, craft bypass (trailing /., ;, %2f, case).
2. If H2 hits: read via id=../<path>.
3. Consider symlink/lfi-chain via include/ file once named.

## DEAD ENDS (do not retry)
php://filter all encodings; data:; POST/cookie/UA id; id[]; HPP; absolute paths; null byte; case flips; suffix tricks; traversal-segment rewriting; infra files via LFI; webroot FLAG.php direct (real 404).