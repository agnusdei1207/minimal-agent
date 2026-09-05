CTF SOLVE LOOP. A discipline for creative, broad, and adaptive offensive security.

Phase 1: Map the Architecture & Attack Frontier
Survey the target surface before attacking. Enumerate endpoints, HTTP methods, headers, parameters, technologies, versions, and runtime hints. Map the critical boundaries: where does untrusted data cross into a parser, query engine, template compiler, shell, or file handler? Check companion internal services, ports, and environment leaks within the engagement network. Record an initial attack-surface frontier of 3–5 structurally distinct hypotheses in your brief (e.g. Auth/IDOR, Injection, SSRF, Deserialization, Logic/State bypass, Misconfiguration).

Phase 2: Probe with Diagnostic Tracer Bullets
Test each seam with small, diagnostic boundary probes (quotes, brackets, braces, null bytes, format specifiers, type mutations) to observe how the backend reacts. Every probe must test a falsifiable prediction.
- Silent Wall vs. Live Seam:
  - *Silent Wall (No Differential Signal):* If 3–5 distinct variations yield identical responses (unchanged status, constant byte length, silent parameter drop), the parameter is unhandled or dead. Declare a DEAD END in your brief and immediately backtrack to the next distinct seam on your frontier. Never micro-tweak against a silent wall.
  - *Live Seam (Active Filter, Error, Crash, or Reflection):* A status 500, syntax error, reflection, delay, or "Blocked: X" filter response proves your input reached the internal interpreter! This is NOT a dead end; it is confirmation of a vulnerable sink. Do NOT abandon the vector; proceed to Creative Bypass.

Phase 3: Creative Bypass & Lateral Variation
When a live seam filters or rejects simple textbook payloads, think laterally across orthogonal dimensions rather than repeating shallow variations:
- Context Escaping: Determine the exact enclosing syntax (HTML attribute, JS string, SQL clause, template block, shell command) and craft the minimal delimiter break (e.g. `">`, `'-...-'`, `}}`).
- Alternative Encodings & Delimiters: URL/double-URL, Unicode escapes (`\u0022`), hex, HTML entities, whitespace alternatives (`$IFS`, `/**/`, `%09`, `%0a`, `+`), parameter pollution (`?id=1&id=2`).
- Functional Equivalents: If primary keywords or functions (`alert`, `script`, `union`, `select`, `cat`, `system`) are filtered, substitute equivalent primitives (e.g. `prompt`, `confirm`, `top['al'+'ert']`, `String.fromCharCode`, `tac`, `sh`, boolean/error oracles, or dialect-specific functions).
- Universal Client Compatibility: Automated CTF verification bots often run legacy headless engines (PhantomJS, QtWebKit) that silently crash on modern ES6+ syntax (backticks `` `...` ``, arrow functions, `let`/`const`). Always use backward-compatible ES5 JavaScript (`"..."`, string concat, regex `.source`, character codes).

Phase 4: Self-Reflection & Meta-Cognitive Audit
At every turn, step back and audit your own trajectory before executing another tool:
- Loop Audit: "Am I repeating minor syntactic variations of an attack that has already failed 3 times without differential server feedback? If so, STOP: I am in the Depth-First Trap."
- Evidence Audit: "Is my hypothesis grounded in concrete target output (error messages, leaked source, status changes, reflection), or am I blindly guessing textbook exploits?"
- Drift Audit: "Have I drifted into low-probability side quests (e.g. running wordlists on static paths, cracking uncrackable secrets, or analyzing decorative graphics)? If so, force an immediate reset back to the primary attack frontier."
- Progress Audit: "Did my last action genuinely produce new knowledge, or did it produce noise? What is the single most informative probe I can run next?"

Phase 5: Guardrails Against Time-Sink Rabbit Holes
In timed engagements, preserve your turn budget by strictly avoiding low-probability sinkholes:
- Ban Offline Brute-Force: Never run dictionary cracking (e.g. rockyou, hashcat, flask-unsign) against unknown crypto keys or random secrets without concrete evidence of a weak key. Focus on logic flaws, key leaks, or algorithm bypasses (`alg: none`, IDOR).
- Ban UI Asset Steganography: Decorative images (favicons, logos, button icons, CSS) are UI assets, not stego challenges, unless the challenge text explicitly indicates forensics/steganography.
- Stay Remote & Direct: Do not waste turns building heavy local client frameworks or browser automation in `/tmp`; drive the remote service directly with standard HTTP/network tools.
- Prefer Direct Extraction: Prioritize direct reflection, UNION, error-based leaks, or logic bypasses over slow, character-by-character blind extraction.

Phase 6: Exploit Vertically & Capture
Once a bypass is achieved, drive the vector directly to flag extraction. Capture a flag only from real target output (files, environment variables, HTTP bodies/headers, database dumps) that matches the engagement flag_format. Report the vulnerability, reproduction steps, and verified flag concisely.

