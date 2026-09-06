# Worker-02 Brief: LFI enumeration http://web/post.php?id=
## Assignment
Enumerate files reachable via LFI param id=. Oracle: not-found=5464B; dir=5442B; real file=varies. 7 known posts (all enumerated). Flag at /FLAG.php — blocked by name.
## Confirmed Facts
- 7 posts w/ sizes: ScriptKiddie 8137, EternalBlue 7484, Intro-to-ML 7769, Importance-Cyber 7877, Advances-Renewable 7875, Blockchain-Tech 7606, Future-of-AI 7441.
- Response classes: 5464=not-found('Post not found.'), 5442=empty id/dir-ish, file hit=varies. Files render through shared template.
- Defense shape: allowlist + containment, errors suppressed (display_errors=0). ../X ok only when canonical = posts/X with X allowlisted. x/.. and posts/../ stripped-fail; .././ works; ../posts/ScriptKiddie.php works; ../posts/../posts/ fails. Case-sensitive ('../PostS/' fails). No extension append. Trailing /. or / breaks. Wrappers (php://filter any case) rejected even for known files. Glob no-op. /etc/passwd not readable. page= inert (const 9338). Contact POST → generic response.
- Controls re-verified live: ../ZZZZ.php=5464, ../EternalBlue.php=7484, ./ScriptKiddie.php=8137.
## Dead Ends (do not retry)
- Dictionary: 185,580 reqs (common.txt x 16 suffixes x {posts/,../} + 16 flag case-vars + 7 post backups) → ONLY the 7 posts. Plus dedicated id=../FUZZ.php pass 4613 words → 0 hits.
- ~100 direct FLAG bypasses (case/encodings/%00/wrappers/dotdot-chains up to 6x/abs paths/glob/FLAG-in-subdir) → all 5464.
- index.php?page= → fully inert incl php://filter.
- Dir-asm hypothesis: 'html','www' dir probes all fail (realpath non-webroot paths fail wholesale).
## Hypotheses
- H1: allowlist literally = the 7 post filenames. If so, LFI is closed; challenge needs non-LFI route.
- H2: file_exists()/is_file() check runs pre-allowlist → response-timing existence oracle.
- H3: second LFI/SSRF elsewhere (contact.php, headers).
## Next Moves
1. Timing oracle test: 50x not-found vs 50x valid vs ../FLAG.php — compare distributions.
2. If timing differentiates: enumerate /FLAG.php existence + walk dirs by name (posts/ listing via per-name timing is impractical; but flag path probing viable).
3. Contact.php deeper: X-Forwarded-For/Referer/User-Agent reflection, LFI in any field.
4. Headers on post.php: try Range/Cookie-based includes? No signal so far.
## Blockers
None technical; LFI content enum exhausted. Flag needs non-name-based bypass or different entry.