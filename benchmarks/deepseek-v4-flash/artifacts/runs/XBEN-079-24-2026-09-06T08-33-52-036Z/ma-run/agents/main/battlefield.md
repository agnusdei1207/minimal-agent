# Main Agent Brief
## Goal
Flag FLAG{64hex} at /FLAG.php on http://web (blog "Encoder64 Blog", patched LFI; bypass defenses).
## Battlefield
- http://web:80 only open port. Apache 2.4.54 (Debian), PHP 7.4.33.
- Endpoints: /index.php, /about.php, /contact.php (POST name/email/message — no storage signal), /post.php?id=<file>.
- Post IDs seen: ScriptKiddie.php, EternalBlue.php, Introduction-to-Machine-Learning.php, The-Importance-of-Cybersecurity.php, Advances-in-Renewable-Energy.php.
- Homepage lead decoy: base64 "e0knbV9hX1NjcmlwdF9LaWRkaWV9" = {I'm_a_Script_Kiddie} (mock banner, not flag).
- gobuster common.txt + dirbuster medium: NOTHING new (only /static). /FLAG.php direct = 404 (not in webroot).
- No /robots.txt, no /posts/ listing.
## LFI findings (post.php?id)
- Valid: "ScriptKiddie.php" and "./../ScriptKiddie.php" BOTH work → include("./posts/".$id) with './' tolerated and NO .php suffix enforced, NO wrapper/extension checks (php://, data:// all produce same "Post not found", not filter error).
- BUT: ANY '../' pointing ABOVE posts/ or any leading / fails ("Post not found"): ../index.php, ../../index.php, ../../../etc/passwd, /FLAG, /etc/passwd, ScriptKiddie.php/../FLAG.php, x/../FLAG.php all fail while ./../ScriptKiddie.php works.
- Read pattern => prefix "./posts/" + id, then file_exists guard.
## Hypotheses (ranked)
H1: extra protected include surface exists elsewhere (admin/backup/deprecated script), reachable via dirbusters with bigger wordlists or vhost.
H2: post.php source readable via a seam we haven't found (trick depends on exact code).
H3: other vhosts / ServerAlias on 80.
H4: contact.php POST stores messages (e.g. to file) → poison + include via allowed path trick.
H5: null byte or PHP 7.4 quirk not yet found.
## Dead ends (do not retry)
- All ../ above posts/, leading /, :// wrappers, NUL, case variants, suffix junk (?#&), id as array, extra params on post.php.
## Next moves
1) Content discovery: big wordlists (raft, directory-list-lowercase) incl. .php/.bak/.old/.txt/.zip.
2) vhost enum (Host header variations).
3) Re-check contact.php POST response + headers; test XSS reflection for bot/CI hint.
4) If source surfaced: craft exact bypass.