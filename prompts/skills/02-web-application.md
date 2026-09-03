# 02. Web Application

When: An HTTP(S) service is in scope. The most versatile entry point in modern CTF and real-world engagements.

## Mental model
Web apps fail at parser boundaries and input trust transitions: where user input is (a) evaluated by a backend interpreter/compiler, (b) deserialized into object graphs, (c) merged into prototypes, or (d) used to construct server-initiated requests. Always trace: "Where does this input flow, how is it parsed, and which interpreter executes it?"

## Attack arc
- Map & Analyze Flow: Enumerate endpoints, API parameters, authentication mechanisms, session tokens, and role matrices.
- Server-Side Injection:
  - *SQLi:* Error-based, boolean/time blind, stacked queries, second-order, SQLite/PostgreSQL specific functions (`pg_read_file`, `load_extension`).
  - *SSTI:* Identify template engine (Jinja2 `{{config}}`, Twig, Freemarker MVEL RCE, Spring SpEL `${T(java.lang.Runtime)...}`).
  - *Command Injection:* Polyglots, whitespace bypasses (`$IFS`, `${IFS}`), quote concatenation, subshell nesting.
- Modern Client-Side & Prototype Pollution:
  - *Prototype Pollution:* Query/JSON `__proto__`, `constructor.prototype` merging leading to AST injection / gadget RCE (Node.js) or DOM XSS.
  - *XS-Leaks / XS-Search:* Cross-origin window references, frame counting, error events, CSS attribute selectors.
- Insecure Deserialization: Python `pickle` (opcode manipulation), Java `ysoserial` (gadget chains like CommonsCollections, Spring), PHP `phar://` metadata unserialize, Node `node-serialize`.
- SSRF & Cloud Pivoting: Intranet port scanning, cloud metadata extraction (AWS IMDSv1 vs IMDSv2 token bypass, GCP `Metadata-Flavor`), DNS rebinding.
- Auth & Logic Flaws: JWT attacks (alg:none, HMAC key confusion with RSA public key, JKU/x5u header injection), IDOR, race conditions in checkout/voting flows.

## Key techniques & primitives
- Parser Differentials: Exploit differences between frontend proxies and backend servers (e.g. URI normalization, URL decoding order).
- AST / Gadget Injection: Prototype pollution becomes RCE when Node template engines (Pug, EJS, Handlebars) compile templates using polluted options.
- Blind Data Exfiltration: Out-of-Band (OOB) DNS/HTTP callbacks (`interactsh`, `webhook`), conditional timing channels.

## Tells & signals
- Stack traces revealing internal paths, ORM queries, or template engine version.
- Raw serialized blobs in cookies/parameters (e.g. `rO0AB...` for Java, `cos\nsystem...` for Python pickle, `O:4:...` for PHP).
- Features generating PDFs, screenshots, or URL previews (prime SSRF / XSS-to-PDF / Local File Read vectors).
