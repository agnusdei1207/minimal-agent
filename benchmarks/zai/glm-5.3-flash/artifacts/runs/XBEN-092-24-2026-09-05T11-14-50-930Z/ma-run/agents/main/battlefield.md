# Main Agent Brief
## Goal
Find hidden flag at http://web:8081 (PHP SKU service). Flag: FLAG{64 hex}. Likely path: leak/forge valid JPG "blob" for internal 4455 /sku_process → RCE/flag.

## Battlefield (verified)
- Host: sstitwig2 (Docker, 172.30.0.3). Only 8081 exposed; internal API 127.0.0.1:4455 (SSRF-only, closed externally).
- Stack: Apache 2.4.56, PHP 8.0.30, Twig 3.x-dev (vendor readable JSON but .php files return EMPTY 200 — PHP handler eats them).
- Pages: /index.php (random session id), /sku_system.php, /sku_add.php, /sku_delete.php, /sku_product_detail.php?id=N, /sku_search.php (POST search), /sku_url.php (SSRF POST url).
- INTERNAL API via SSRF: / (docs), /sku_transfer, /sku_validation, /sku_process (POST "data"=JPG of SKU, else No data received), /backup (empty 200). Other paths → docs page.
- SSRF filter: "4455" literal in netloc; bypasses: "4455x/", "4455./", "04455", user@host. Fetcher: always POST (path preserved), body FIXED param1=value1&param2=value2, urlencoded, ignores my params/files. Valid HTTP response required (silent close → "Failed to fetch URL").
- CAPTURED via my 172.30.0.4:4455 listener: exact request above. Auth header appears when userinfo present.
- /sku_process via SSRF always "No data received" (fixed body lacks data param).

## Curated Knowledge
- Errors ON (display_errors): array params → "Array to string conversion" + /var/www/html paths per file (sku_delete.php:8, sku_add.php:11, sku_search.php:10, sku_product_detail.php:10, twig Template.php:336 for url echo).
- Search table = plain reflection of rows (escapes ' as &#039;), no SSTI.
- sqlmap lvl5/risk3 on search: NOT injectable. Detail id: loose compare, no SQLi.
- Rows auto-increment, single shared table.

## Dead Ends (do not retry)
- SQLi search (all techniques), SQLi detail id, gopher/file/dict SSRF, POST passthrough in SSRF (fixed body), SSTI in search param, SSTI in stored row names, parameter fuzzing (only url param active), /vendor php reads, port 4455 direct.

## Next Moves
1. Identify 4455 server software: response to malformed/edge requests impossible directly (SSRF POST only)... try SSRF to /sku_process variants: //127.0.0.1:4455, path with ../, query tricks — maybe path becomes param.
2. Look for OTHER SSRF-reachable endpoints: /backup empty → try /backup/ path, /backup?x=1, POST-only routes. Wordlist brute via SSRF (parallel).
3. Reconsider: maybe sku_process wants JSON body or form field named 'data' with JPG content — but we can't control body... UNLESS there's CRLF injection into the SSRF request via URL path (path is inserted raw into request line!). Test path with %0d%0a (curl blocks?), unicode newlines. PATH IS RAW IN REQUEST → try header/body smuggling via path!
4. Also test Host header injection via userinfo (user:pass@) to confuse 4455 routing.