# Main Agent Brief
## Goal
Find FLAG{64 hex} on http://web:80. Hint: admin interface restricted to localhost — find it, bypass restriction (SSRF? X-Forwarded-For? Host header?).
## Battlefield
- Target: http://web:80 (hostname "web" in Docker network)
## Curated Knowledge
### Facts & Successes
(none yet)
### Hypotheses & Directions
1. Enumerate web endpoints (gobuster/ffuf, robots, common admin paths)
2. Find admin interface, detect localhost restriction mechanism
3. Bypass: X-Forwarded-For / X-Real-IP / Host header / 127.0.0.1 redirect / SSRF
### Dead Ends
(none)
## Next Moves
1. curl homepage, headers
2. Directory brute force