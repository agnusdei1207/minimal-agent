# 15. HTTP Intercept & Replay

When: Crafting bespoke raw HTTP requests, debugging multi-stage web flows, testing concurrency/race conditions, or exploiting protocol smuggling.

## Mental model
Web browsers and standard HTTP libraries abstract and sanitize request structures. High-tier web exploitation requires exact, byte-level control over raw headers, delimiters, chunk encodings, and multiplexed streams: observing how backend proxies and application servers diverge when interpreting ambiguous protocol states.

## Attack arc
- Direct Scripting vs. Intercept Proxy:
  - *Direct Scripting (Python `httpx`, `requests`, raw sockets):* Use when the request schema is known and exact byte manipulation (e.g. malformed newlines `\r\n`) or concurrency is needed.
  - *Intercept Proxy (`mitmproxy`, `caido`, custom proxy):* Run a persistent interceptor inside a shared tmux session to capture complex browser/API flows, inspect anti-CSRF tokens, and record session states. To capture `agent-browser` traffic on demand (INTENT-0005): launch `tmux new-session -d -s proxy "mitmdump -p 8080 --save-stream-file /tmp/traffic.flow"`, set `export http_proxy=http://127.0.0.1:8080`, and drive `agent-browser open <url>` (browser traffic routes through the proxy while local CDP is excluded).
- HTTP Request Smuggling (HRS):
  - Exploit discrepancies between frontend reverse proxies and backend servers when determining request boundaries:
    - CL.TE: Frontend uses `Content-Length`, Backend uses `Transfer-Encoding: chunked`.
    - TE.CL: Frontend uses `Transfer-Encoding`, Backend uses `Content-Length`.
    - TE.TE: Obfuscate `Transfer-Encoding` header (e.g. `Transfer-Encoding: xchunked`, `Transfer-encoding: [tab]chunked`) to make one server ignore it.
  - HTTP/2 Request Smuggling (H2.CL / H2.TE): Frontend speaks HTTP/2, downgrades to HTTP/1.1 backend; inject `\r\n` inside HTTP/2 pseudo-headers or headers.
  - *Exploitation Impact:* Hijack other users' requests, poison web caches, bypass frontend authentication filters, exfiltrate sensitive request headers.
- Concurrency & Single-Packet Race Conditions:
  - Exploit Time-Of-Check to Time-Of-Use (TOCTOU) logic flaws (e.g. redeeming gift cards multiple times, double spending, voting bypasses).
  - Use HTTP/2 multiplexing or HTTP/1.1 TCP connection warming to send 20–50 requests synchronized to arrive within the same millisecond.
- WebSocket Hijacking & Frame Tampering:
  - Test for Cross-Site WebSocket Hijacking (CSWSH) when handshake lacks CSRF tokens or validates `Origin` improperly.
  - Tamper with binary/JSON WebSocket frames directly via Python `websockets`.

## Key techniques & primitives
- CL.TE Smuggling Template (Raw Socket):
  ```http
  POST / HTTP/1.1\r\n
  Host: target.com\r\n
  Content-Length: 13\r\n
  Transfer-Encoding: chunked\r\n
  \r\n
  0\r\n
  \r\n
  GPOST /admin HTTP/1.1\r\n
  ...
  ```
- Single-Packet Race: Synchronize `send()` across multiple parallel connections by holding the final byte of each request and releasing them simultaneously.

## Tells & signals
- `504 Gateway Timeout` or `400 Bad Request` following a request with both `Content-Length` and `Transfer-Encoding` = smuggling candidate.
- Race conditions: Balance decreases below 0 or coupon registers multiple applications on concurrent requests.
- WebSockets exchanging sensitive state without session-bound challenge tokens.
