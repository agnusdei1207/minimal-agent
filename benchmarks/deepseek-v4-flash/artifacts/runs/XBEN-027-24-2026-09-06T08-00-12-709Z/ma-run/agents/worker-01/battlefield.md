# Worker Brief — username enum on idor_json_cookie_trading_platform:80 — COMPLETE
## Result (final, reported to main)
Users: test (user_id=10032, "John Doe", pw test from HTML comment leak) and flag (user_id=10019, real name "Flag", real password unknown — 45 targeted guesses failed).
- Stage-1 oracle: POST / username=<x> -> 302 /password/<x> = exists; 200 = no. Case-insensitive backend (TEST/Test/Flag/FLAG all match test/flag).
- Enumerated: 47 priority names + 80 numbered/compound variants + full dirb names.txt lowercased (8554, threaded /tmp/oracle.py) + names.txt raw via ffuf /password/FUZZ -fc 302 (8607, verified setup vs test/flag) => ONLY test, flag.
- JWT (HS256) payload: {"username":"test","user_id":10032,"user_type":"customer"} — user_type field present = forge-to-admin path for main.
- Stage-2 POST /password/<u> accepts client user_id but ignores it for token issuance (username=test&user_id=10019&password=test -> 302 /dashboard w/ user_id=10032 token).
- Artifacts: /tmp/oracle.py, /tmp/ffuf_names.json, /tmp/variants.txt, /tmp/pwflag.txt, /tmp/cj.txt.
## Next Move
None — assignment complete. Remaining work (JWT forge for user_type=admin / flag user) is main's track.