#!/usr/bin/env python3
"""Scripted, CORS-enabled mock of the spoo.me API for tests/wasm.rs.

The wasm-bindgen-test page runs on a different origin than this server, so
every response carries permissive CORS headers and OPTIONS preflights are
answered. State: /api/v1/urls/retry alternates 429 (with Retry-After: 1)
and 200, which is what the retry test relies on.
"""

import json
from http.server import BaseHTTPRequestHandler, HTTPServer

PORT = 18300
STATE = {"retry_hits": 0}


class Handler(BaseHTTPRequestHandler):
    def _reply(self, status, body=None, extra_headers=None):
        self.send_response(status)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Headers", "*")
        self.send_header("Access-Control-Allow-Methods", "*")
        self.send_header("Access-Control-Expose-Headers", "*")
        for name, value in (extra_headers or {}).items():
            self.send_header(name, value)
        data = b""
        if body is not None:
            data = json.dumps(body).encode()
            self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_OPTIONS(self):
        self._reply(204)

    def do_GET(self):
        if self.path == "/api/v1/urls/plain":
            self._reply(200, {"id": "plain", "password_set": False})
        elif self.path == "/api/v1/urls/retry":
            STATE["retry_hits"] += 1
            if STATE["retry_hits"] % 2 == 1:
                self._reply(
                    429,
                    {"error": "slow down", "code": "rate_limit_exceeded"},
                    {"Retry-After": "1"},
                )
            else:
                self._reply(200, {"id": "retry", "password_set": False})
        else:
            self._reply(404, {"error": "no such URL", "code": "not_found"})

    def log_message(self, *args):
        pass


if __name__ == "__main__":
    print(f"wasm mock listening on 127.0.0.1:{PORT}", flush=True)
    HTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
