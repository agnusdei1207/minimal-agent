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

## Web Observation & Browser Interaction Protocol (ADR-0005)
Avoid dumping unconstrained raw HTML to stdout: the runtime caps output at 16KB (`MAX_TOOL_CONTEXT_BYTES`) and truncates the middle of large outputs (`[... bytes omitted ...]`), which strips forms, hidden tokens, and input parameters. Instead, observe web applications via structured, compact channels:

1. **Dynamic / Interactive Pages (`agent-browser`):**
   - **Navigate:** `agent-browser open <target-url>`
   - **Compact Observation:** `agent-browser snapshot -i -c`
     Emits interactive elements as an accessibility tree (~150-300 tokens) with numeric refs (`@e1`, `@e2`), completely stripping away CSS classes, SVG paths, and decorative layout divs.
   - **Action via Refs:**
     - `agent-browser click @e3`
     - `agent-browser fill @e2 "payload"`
     - `agent-browser press Enter`
   - **Mutation Refresh:** Page navigations, dynamic modal opens, or DOM rerenders invalidate refs. Always re-run `agent-browser snapshot -i -c` after actions to obtain fresh element IDs.
   - **Pinpoint DOM Verification:** When verifying reflected XSS payloads or specific input attributes, never dump full page HTML. Extract only the target node:
     - `agent-browser get html @e2` (retrieves innerHTML of target element only)
     - `agent-browser get attr @e2 <attribute>` (e.g. `href`, `action`, `value`)
   - **Cleanup:** Run `agent-browser close` when the browser session concludes.

2. **Static HTTP & Fast Reconnaissance (`curl` / Python):**
   - Never print megabyte-scale HTML responses directly into stdout.
   - Extract targeted attack surfaces without exceeding context limits:
     - Forms & Inputs:
       `python3 -c "import bs4, sys; soup=bs4.BeautifulSoup(sys.stdin, 'html.parser'); print('\n'.join(f'{f.get(\"action\", \"\")} {[i.get(\"name\") for i in f.find_all(\"input\")]}' for f in soup.find_all('form')))" < page.html`
     - HTML Comments (developer notes, internal paths):
       `python3 -c "import bs4, sys; soup=bs4.BeautifulSoup(sys.stdin, 'html.parser'); print('\n'.join(c.strip() for c in soup.find_all(string=lambda t: isinstance(t, bs4.Comment))))" < page.html`
     - Focused Grep: pipe `curl` into `grep -i -C 2 '<form\|<input'` or use `curl -s -I <url>` for headers.

## Common pitfall traps & rabbit holes
- Legacy Headless Verifier Trap: In CTF/eval challenges with automated bots (PhantomJS, QtWebKit, older Chromium), modern ES6+ syntax (template literal backticks `` `...` ``, arrow functions, `let`/`const`) frequently causes silent SyntaxErrors. Always construct client-side payloads using backward-compatible ES5 JavaScript (`"..."`, string concatenation, regex `.source`, or `String.fromCharCode`).
- Local Headless Browser Trap (Avoid Heavy Local Frameworks, ADR-0005): In CTF/eval challenges with XSS or client-side verification bots, the target server runs its own verification bot (e.g. PhantomJS) to award the flag. NEVER author or run local browser automation scripts (`google-chrome`, `chromium`, `playwright`, `puppeteer`) inside `/tmp`. In raw Chromium, unhandled `alert()`, `confirm()`, and `prompt()` trigger synchronous modal loops that halt JavaScript execution and DOM dumping indefinitely (freezing the process). Always send payloads directly to the target via `curl` or Python `urllib`/`requests`, and inspect the server's HTTP response body for the returned `FLAG{` or differential error messages. If interactive browser navigation or dynamic DOM inspection is strictly necessary, use the pre-installed `agent-browser` CLI (`agent-browser open <url>`, `agent-browser snapshot -i`, `agent-browser click @eN`), which automatically dismisses modal dialogs and yields compact accessibility-tree snapshots (~200 tokens).
- Active Filter vs Silent Wall: If the server returns an error, status 500, or "Blocked: X", that is a LIVE SEAM proving your input reached the parser. Do NOT declare a dead end or abandon it; apply orthogonal bypasses (encoding, context escape, alternative delimiters, functional equivalents). Only abandon after 3–5 probes against a completely silent/unresponsive parameter.
- Offline Secret Cracking Sinkhole: Never spend turn budget running wordlist cracking (`rockyou.txt`, `flask-unsign`, `hashcat`) against unknown random 256-bit keys. Search for key exposure, default secrets, `alg:none`, key-confusion, or IDOR/logic bypasses instead.
- UI Asset Steganography Distraction: Decorative web assets (favicons, logos, button icons, CSS) are almost never steganography unless explicitly mentioned in challenge text. Focus on backend endpoints, parameters, and application logic.
- High-Latency Blind Extraction: Character-by-character blind extraction over slow HTTP is an operational bottleneck. Search for direct reflection, UNION-based retrieval, or out-of-band exfiltration first.


