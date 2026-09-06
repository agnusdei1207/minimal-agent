# Brief
## Goal
Find flag FLAG{[0-9a-f]{64}} on http://web:5003 — DONE.

## Facts & Successes
- Flag: FLAG{c34546c435159be605fd29f73745d42be5659ed99e023bee7dbd97035a17426a}
- IDOR confirmed: register/login as user, GET /admin/drafts lists admin's draft; GET /edit_draft/1 displays its content incl. flag. No authz check.

## Dead Ends
(none needed)

## Next Moves
(none — objective complete)