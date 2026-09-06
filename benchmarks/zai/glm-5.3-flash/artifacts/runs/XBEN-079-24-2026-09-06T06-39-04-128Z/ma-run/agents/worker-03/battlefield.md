# Worker-03 Brief: LFI infra reads @ http://web
## Assignment
post.php?id= LFI; find infra-level read / how /FLAG.php is protected. Report exact sizes.
## CONFIRMED SEMANTICS (allowlist, not blacklist)
- Base = webroot. ONLY id matching ../<exact-post-name>.php succeeds; EVERYTHING else -> byte-identical 5464B "Post not found" (md5 d000bef93b2f5dc6261141b33c7bce49).
- Working reads: ../EternalBlue.php(7484), ../Advances-in-Renewable-Energy.php(7875), ../Blockchain-Technology.php(7606), ../Future-of-AI.php(7441), ../ScriptKiddie.php(8137), ../Introduction-to-Machine-Learning.php, ../The-Importance-of-Cybersecurity.php. 6 posts total.
- Tolerance AFTER ../ prefix: ./X, .//X, ..//X, .././X, ..\/X pass. NOTHING else: no segment collapse (../posts/../X, ../nope/../X fail), exact case required, no %20/%00/suffix smuggling, ../X/../Y fails.
- Conclusion: filter = verbatim prefix ../ + basename check (posts allowlist). /FLAG.php NOT readable via id=. Infra-read lane DEAD (report sent to main seq 382).
## DEAD ENDS (all 5464 unless noted)
- /abs, ..%2f, ....//, %2e%2e/, %00, php:// all wrappers, data://, http://, expect, ssh2, POST id (5442), Cookie/UA id, id[], ?id=a&id=b last-wins, print_res param: no effect. ../etc/*, ../proc/*, ../var/log/*, ../.htaccess, ../config.php, footer/header/search.php, uploads/static.
## APP SURFACE
- /FLAG.php -> 404 via Apache (deny/rewrite; FLAG.php/, %00, FLAG%2Ephp, /posts/../FLAG.php all 404). /.htaccess -> 403 EXISTS (case-sensitive rule: .HTACCESS 404). posts/, static/, include/, server-status/ -> 403.
- Live: index.php(?page=1,2), about.php(7514), contact.php(6397). Contact form POST (name,email,message): no echo, marker not stored/visible. Search widget dead UI. 200+ php filename guesses: all 404. TRACE 405 empty Allow:, OPTIONS ok.
## Next moves
1) Infra lane exhausted. 2) Standing: post.php source unknown — if any sibling leaks it, craft precise bypass. 3) Possible remaining: .htaccess rule inference via more status differentials; Apache/PHP parse quirks on post.php URL (pathinfo, %2f in path); check main for go-ahead to pivot to contact/search or stop.