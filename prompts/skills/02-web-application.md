# 02. Web Application

When: An HTTP(S) service or web API is in scope. The most versatile entry point in modern CTF and real-world engagements.

## Mental model
Web applications fail at parser boundaries, input trust transitions, and state assumptions:
1. Where user input is evaluated by a backend interpreter (SQL, template engine, OS shell, regex engine).
2. Where data crosses deserialization boundaries into object graphs or memory structures.
3. Where differences exist between how a frontend proxy normalizes a request vs how the backend server routes it.
4. Where state is assumed rather than verified (race conditions, IDOR, authorization bypasses).

Always ask: "Where does this input flow, which interpreter executes it, and what seam exists between the components?"

## Build the feedback loop
Be aggressive. Be creative. Refuse to give up.
If you have a sharp differential signal, you will find the exploit. If you do not have one, blind guessing will stall your engagement.

Ways to establish a differential signal:
1. **Status and length diffs:** Inject boundary characters (`'`, `"`, `\`, `{{7*7}}`, `${IFS}`, `%00`, `<svg>`) and compare HTTP response code, content length, and response time against a baseline request.
2. **Error and exception induction:** Force internal stack traces, database syntax errors, or debug modes through malformed types, array parameters (`param[]`), or missing fields.
3. **Behavioral divergence:** Test whether modifying an input causes backend delays (timing probes like `sleep` or heavy operations) or triggers different application states.
4. **Header and method probes:** Test alternative HTTP verbs (`HEAD`, `OPTIONS`, `PUT`, `PATCH`) and routing headers (`X-Forwarded-For`, `X-Original-URL`, `X-Rewrite-URL`, `Host`).

## Creative exploitation arcs
- **Interpreter Injection:**
  - *SSTI:* Identify the engine (`{{7*7}}`, `${7*7}`, `<%= 7*7 %>`). Explore engine internals, globals, filter bypasses, and MRO/prototype traversal to reach command execution.
  - *SQLi:* Error-based, boolean/time differential, stacked queries, SQLite/PostgreSQL specific functions (`pg_read_file`, `load_extension`).
  - *Command Injection:* Polyglots, whitespace alternatives (`$IFS`, `${IFS}`), quote concatenation, subshell expansions.
- **Secondary Channels & Chaining:**
  - When direct injection is blocked or rendered invisibly (e.g. an include file that returns blank), think laterally: where does user-controlled data get stored? Check accessible logs (Apache access.log poisoning), session files, temporary uploads, or environment variables.
  - Exploit chaining: combine a minor file read (LFI) with log poisoning or session upload to achieve full remote code execution.
- **Parser Differentials & Smuggling:**
  - Mismatches in URL decoding order, path traversal normalization (`....//`), and HTTP request smuggling (CL.TE, TE.CL) allow bypassing frontend path-based ACLs to reach protected internal endpoints.
- **State & Logic Flaws:**
  - Race conditions in multi-step transactions (probe with concurrent parallel requests).
  - Parameter tampering, IDOR on object identifiers, and privilege escalation via role matrix probing.

## Tells & signals
- Small differences in response body length or status codes on boundary characters indicate active parsing.
- Stack traces or error banners revealing backend language, framework, or dependency versions.
- Features processing user-supplied URLs, files, images, or documents (prime vectors for SSRF, file upload RCE, and XXE).

## Common pitfall traps & rabbit holes
- Legacy Headless Verifier Trap: In CTF/eval challenges with automated bots (PhantomJS, QtWebKit, older Chromium), modern ES6+ syntax (template literal backticks `` `...` ``, arrow functions, `let`/`const`) frequently causes silent SyntaxErrors. Always construct client-side payloads using backward-compatible ES5 JavaScript (`"..."`, string concatenation, regex `.source`, or `String.fromCharCode`).
- Active Filter vs Silent Wall: If the server returns an error, status 500, or "Blocked: X", that is a LIVE SEAM proving your input reached the parser. Do NOT declare a dead end or abandon it; apply orthogonal bypasses (encoding, context escape, alternative delimiters, functional equivalents). Only abandon after 3–5 probes against a completely silent/unresponsive parameter.
- Offline Secret Cracking Sinkhole: Never spend turn budget running wordlist cracking (`rockyou.txt`, `flask-unsign`, `hashcat`) against unknown random 256-bit keys. Search for key exposure, default secrets, `alg:none`, key-confusion, or IDOR/logic bypasses instead.
- UI Asset Steganography Distraction: Decorative web assets (favicons, logos, button icons, CSS) are almost never steganography unless explicitly mentioned in challenge text. Focus on backend endpoints, parameters, and application logic.
- High-Latency Blind Extraction: Character-by-character blind extraction over slow HTTP is an operational bottleneck. Search for direct reflection, UNION-based retrieval, or out-of-band exfiltration first.


