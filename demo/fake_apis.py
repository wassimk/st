"""Fake Slack and GitHub APIs for the README demo.

Serves on 127.0.0.1:19200. Point st at it with:

    ST_SLACK_API_URL=http://127.0.0.1:19200/slack
    ST_GITHUB_API_URL=http://127.0.0.1:19200/graphql

Every Slack method returns ok and every GraphQL mutation succeeds, so st prints
its normal output without touching a real account.
"""

import json
from http.server import BaseHTTPRequestHandler, HTTPServer

PORT = 19200


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        self.rfile.read(int(self.headers.get("Content-Length", 0)))

        if self.path.startswith("/slack/"):
            body = {"ok": True}
        elif self.path == "/graphql":
            body = {"data": {"changeUserStatus": {"status": None, "clientMutationId": None}}}
        else:
            self.send_error(404)
            return

        payload = json.dumps(body).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, *args):
        pass


HTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
