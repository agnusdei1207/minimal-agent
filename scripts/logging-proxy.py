#!/usr/bin/env python3
"""OpenAI-compatible logging proxy (Python stdlib only, keyless-safe).

Listens on host ``localhost:8080`` (binds ``127.0.0.1:8080`` by default) and
forwards ``POST /chat/completions`` to the upstream OpenAI-compatible base URL
taken ONLY from the runtime environment variable ``OPENAI_BASE_URL``
(default ``https://openrouter.ai/api/v1``). No API key is hardcoded, stored,
or written anywhere by this process.

Container wiring (documented, NOT bound here):
    container -> http://host.docker.internal:8080/chat/completions
Host wiring:
    proxy listens on 127.0.0.1:8080; the harness ``.env`` points
    ``OPENAI_BASE_URL`` at the proxy while the proxy process itself holds the
    real gateway key ONLY from outside-repo env/file (e.g. shell env or a
    key file outside the repo). NEVER commit a proxy key file. Main branch
    only.

Minimal-agent request shape (see src/provider.rs):
    POST {base}/chat/completions with Bearer auth and JSON body
    {model, messages, tools?, tool_choice?, stream:true,
     include_reasoning:true, max_completion_tokens, temperature?}.
    SSE responses terminate with ``data: [DONE]``; non-stream JSON is also
    passed through untouched.

Logging policy (never log secrets or bodies):
    * The full ``Authorization`` value is NEVER logged. Only a masked form is
      recorded: first 4 chars + ``...`` + last 2 chars at most
      (e.g. ``Bearer sk-t...78``); short/empty values become ``Bearer <masked>``
      or ``-``. Forwarding passes the header through opaquely.
    * Request/response BODIES are never logged. Only metadata is logged:
      timestamp, method, path, model, stream flag, request chunk count,
      response chunk count, ``[DONE]`` presence, status, duration.
    * The proxy holds no key itself; it forwards whatever the client sent.

Usage (no live calls are made by importing this module):
    python scripts/logging-proxy.py [--host 127.0.0.1] [--port 8080]
        [--log-file PATH]
    OPENAI_BASE_URL env var selects upstream (default below). The real key
    travels only in the forwarded ``Authorization`` header, never in logs.

Keyless guard: importing or ``--help`` performs zero network I/O and creates
no ``.env`` file.
"""

from __future__ import annotations

import argparse
import datetime
import json
import os
import sys
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

DEFAULT_UPSTREAM = "https://openrouter.ai/api/v1"
DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 8080

# Container-facing URL (documentation only; the server binds DEFAULT_HOST).
CONTAINER_URL = "http://host.docker.internal:8080"

ALLOWLIST = {
    "/chat/completions": "/chat/completions",
    "/v1/chat/completions": "/chat/completions",
}

HOP_BY_HOP = frozenset({
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
})


def mask_auth(value: str | None) -> str:
    """Mask an Authorization header value; never reveal the full credential."""
    if value is None:
        return "-"
    text = value.strip()
    if not text:
        return "-"
    scheme = ""
    token = text
    if " " in text:
        scheme, token = text.split(" ", 1)
        token = token.strip()
        if not token:
            return "%s <masked>" % scheme if scheme else "<masked>"
    if len(token) <= 6:
        masked = "<masked>"
    else:
        masked = "%s...%s" % (token[:4], token[-2:])
    return ("%s %s" % (scheme, masked)) if scheme else masked


def resolve_upstream(env: dict[str, str] | None = None) -> str:
    """Return upstream base URL from runtime env ONLY (no hardcoded key)."""
    source = env if env is not None else os.environ
    base = (source.get("OPENAI_BASE_URL") or "").strip() or DEFAULT_UPSTREAM
    return base.rstrip("/")


def build_upstream_url(base_url: str, suffix: str) -> str:
    """Join an upstream base URL with an allowlisted path suffix."""
    return base_url.rstrip("/") + suffix


def normalize_path(raw_path: str) -> str | None:
    """Strip query string; return allowlisted upstream suffix or None."""
    path = raw_path.split("?", 1)[0]
    return ALLOWLIST.get(path)


def extract_model(body: bytes) -> str:
    """Best-effort model extraction from a JSON request body (metadata only)."""
    if not body:
        return "-"
    try:
        payload = json.loads(body.decode("utf-8"))
    except Exception:
        return "-"
    if isinstance(payload, dict):
        model = payload.get("model")
        if isinstance(model, str) and model:
            return model
    return "-"


def extract_stream_flag(body: bytes) -> str:
    """Best-effort stream flag extraction; returns 'true'/'false'/'-'."""
    if not body:
        return "-"
    try:
        payload = json.loads(body.decode("utf-8"))
    except Exception:
        return "-"
    if isinstance(payload, dict) and "stream" in payload:
        return "true" if bool(payload.get("stream")) else "false"
    return "-"


def count_request_chunks(body: bytes) -> int:
    """Metadata helper: 1 chunk if a body was sent, else 0 (bodies never logged)."""
    return 1 if body else 0


def count_sse_chunks(payload: bytes) -> tuple[int, bool]:
    """Count ``data:`` lines and ``[DONE]`` presence in an SSE payload.

    Operates on bytes already relayed; the payload itself is never logged,
    only the returned counts.
    """
    chunks = 0
    done = False
    for raw_line in payload.split(b"\n"):
        text = raw_line.decode("utf-8", "replace").strip()
        if text.startswith("data:"):
            chunks += 1
            if text == "data: [DONE]":
                done = True
    # Trailing terminator without newline (defensive; loop above misses it
    # only when the payload has no trailing newline at all).
    tail = payload.decode("utf-8", "replace").strip().splitlines()
    if tail and tail[-1].strip() == "data: [DONE]":
        done = True
    return chunks, done


def format_log(now: str, method: str, path: str, model: str, stream: str,
               req_chunks: int, status: int, duration_ms: int,
               resp_chunks: int, done: bool, auth: str) -> str:
    """Format one metadata-only log line (no bodies, masked auth)."""
    return (
        "%s method=%s path=%s model=%s stream=%s req_chunks=%d "
        "status=%d duration_ms=%d resp_chunks=%d done=%s auth=%s"
        % (now, method, path, model, stream, req_chunks, status,
           duration_ms, resp_chunks, "true" if done else "false", auth)
    )


def emit_log(server: ThreadingHTTPServer, line: str) -> None:
    """Write one log line to stdout and optionally to the log file."""
    print(line, flush=True)
    log_file = getattr(server, "proxy_log_file", None)
    if log_file:
        try:
            with open(log_file, "a", encoding="utf-8") as fh:
                fh.write(line + "\n")
        except OSError as exc:
            print("proxy log-file write failed: %s" % exc,
                  file=sys.stderr, flush=True)


class ProxyHandler(BaseHTTPRequestHandler):
    server_version = "LoggingProxy/1.0"

    def log_message(self, fmt: str, *args: object) -> None:  # quiet chatter
        sys.stderr.write("proxy: " + (fmt % args) + "\n")

    def _forward(self, method: str) -> None:
        raw_path = self.path
        suffix = normalize_path(raw_path)
        started = time.monotonic()
        now = datetime.datetime.now(datetime.timezone.utc).isoformat()
        masked = mask_auth(self.headers.get("Authorization"))

        if suffix is None:
            body = b'{"error":"not found (proxy allowlist: POST /chat/completions)"}\n'
            self.send_response(404)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            try:
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                pass
            emit_log(self.server, format_log(
                now, method, raw_path.split("?", 1)[0], "-", "-",
                0, 404, self._elapsed_ms(started), 0, False, masked))
            return

        length = int(self.headers.get("Content-Length") or 0)
        req_body = self.rfile.read(length) if length > 0 else b""
        model = extract_model(req_body)
        stream_flag = extract_stream_flag(req_body)
        req_chunks = count_request_chunks(req_body)

        upstream = self.server.proxy_upstream.rstrip("/")
        target_url = build_upstream_url(upstream, suffix)

        forward_headers: dict[str, str] = {}
        auth_raw = self.headers.get("Authorization")
        for key, value in self.headers.items():
            low = key.lower()
            if low in ("host", "content-length") or low in HOP_BY_HOP:
                continue
            if low == "authorization":
                continue  # set opaquely below, logged only masked
            forward_headers[key] = value
        if auth_raw is not None:
            forward_headers["Authorization"] = auth_raw  # opaque passthrough
        forward_headers.setdefault("Accept", "*/*")
        if method == "POST":
            forward_headers.setdefault("Content-Type", "application/json")

        req = urllib.request.Request(
            target_url,
            data=req_body if method == "POST" else None,
            headers=forward_headers,
            method=method,
        )
        try:
            with urllib.request.urlopen(req, timeout=300) as resp:
                status = resp.status
                content_type = resp.headers.get(
                    "Content-Type", "application/json")
                self.send_response(status)
                self.send_header("Content-Type", content_type)
                self.send_header("Cache-Control", "no-cache")
                self.send_header("Connection", "close")
                self.end_headers()
                resp_chunks, done = self._relay(resp)
        except urllib.error.HTTPError as exc:
            status = exc.code
            try:
                err_body = exc.read()
            except Exception:
                err_body = b"upstream error"
            if not err_body:
                err_body = b"upstream error"
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(err_body)))
            self.send_header("Connection", "close")
            self.end_headers()
            try:
                self.wfile.write(err_body)
            except (BrokenPipeError, ConnectionResetError):
                pass
            resp_chunks, done = 0, False
        except Exception as exc:  # upstream unreachable etc.
            status = 502
            err_body = json.dumps(
                {"error": "proxy upstream failed: %s" % type(exc).__name__}
            ).encode()
            try:
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(err_body)))
                self.send_header("Connection", "close")
                self.end_headers()
                self.wfile.write(err_body)
            except (BrokenPipeError, ConnectionResetError):
                pass
            resp_chunks, done = 0, False

        emit_log(self.server, format_log(
            now, method, suffix, model, stream_flag, req_chunks,
            status, self._elapsed_ms(started), resp_chunks, done, masked))

    def _relay(self, resp) -> tuple[int, bool]:
        """Relay upstream bytes (JSON or SSE) untouched; count metadata only."""
        chunks = 0
        done = False
        buf = b""
        is_sse = "text/event-stream" in (
            resp.headers.get("Content-Type") or "")
        raw_all = bytearray()
        while True:
            piece = resp.read(4096)
            if not piece:
                break
            raw_all += piece
            try:
                self.wfile.write(piece)
            except (BrokenPipeError, ConnectionResetError):
                break
            if is_sse:
                buf += piece
                while b"\n" in buf:
                    line, buf = buf.split(b"\n", 1)
                    text = line.decode("utf-8", "replace").strip()
                    if text.startswith("data:"):
                        chunks += 1
                        if text == "data: [DONE]":
                            done = True
        # Non-SSE JSON passthrough: still report chunk metadata (0/[DONE]=false).
        # SSE tail without trailing newline:
        if is_sse and buf.strip() == b"data: [DONE]":
            if not done:
                chunks += 1
            done = True
        try:
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass
        return chunks, done

    @staticmethod
    def _elapsed_ms(started: float) -> int:
        return int((time.monotonic() - started) * 1000)

    def do_POST(self) -> None:
        self._forward("POST")

    def do_GET(self) -> None:  # allowlist rejects non-POST paths with 404
        self._forward("GET")


def parse_args(argv=None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="OpenAI-compatible logging proxy (stdlib only).")
    parser.add_argument("--host", default=DEFAULT_HOST,
                        help="Listen host (default: %(default)s)")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT,
                        help="Listen port (default: %(default)s)")
    parser.add_argument("--log-file", default=None,
                        help="Append metadata log lines to PATH")
    parser.add_argument("--upstream", default=None,
                        help="Override upstream base URL "
                             "(default: $OPENAI_BASE_URL or %s)"
                             % DEFAULT_UPSTREAM)
    return parser.parse_args(argv)


def main(argv=None) -> int:
    args = parse_args(argv)
    upstream = (args.upstream or resolve_upstream())
    server = ThreadingHTTPServer((args.host, args.port), ProxyHandler)
    server.proxy_upstream = upstream
    server.proxy_log_file = args.log_file
    print("logging-proxy listening on %s:%d -> %s%s"
          % (args.host, args.port, upstream,
             (" (log-file: %s)" % args.log_file) if args.log_file else ""),
          flush=True)
    print("container URL: %s (documented; not bound here)" % CONTAINER_URL,
          flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
