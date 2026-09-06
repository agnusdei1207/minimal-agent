# Battlefield Note
## Goal
Capture FLAG{64 hex} at http://web:5003. Company portal, employee creds emp:emppassword, admin protected with 2FA (claims). Must bypass 2FA / authz to reach admin dashboard.

## Facts
- Target: http://web:5003
- Known creds: emp/emppassword
- App: company portal with employee + admin dashboards; admin claims 2FA.

## Attempts (by vector)
- (pending recon)

## Dead Ends
- none yet

## Next Moves
1. Recon: fetch homepage, headers, routes, JS.
2. Login as emp, inspect session/cookies, discover endpoints (admin).
3. Test admin auth flow & 2FA bypass vectors.