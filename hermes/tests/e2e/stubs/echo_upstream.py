"""An upstream for the proxy test: it listens on the socket of the API and answers each request with its headers.

Usage: python3 echo_upstream.py <socket>. Only the standard library, so it runs in the plain Python image.
"""

import json
import os
import socketserver
import sys
from http.server import BaseHTTPRequestHandler

SOCKET = sys.argv[1] if len(sys.argv) > 1 else "/run/hermes/api.sock"


class Echo(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def answer(self) -> None:
        length = int(self.headers.get("content-length") or 0)
        if length:
            self.rfile.read(length)
        headers = [[name.lower(), value] for name, value in self.headers.items()]
        body = json.dumps({"method": self.command, "path": self.path, "headers": headers}).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    do_GET = answer
    do_POST = answer

    def address_string(self) -> str:
        return "unix"

    def log_message(self, format: str, *args: object) -> None:
        return


class Server(socketserver.ThreadingMixIn, socketserver.UnixStreamServer):
    daemon_threads = True


if os.path.exists(SOCKET):
    os.unlink(SOCKET)
previous = os.umask(0o177)
server = Server(SOCKET, Echo)
os.umask(previous)
os.chmod(SOCKET, 0o600)
print("listening", flush=True)
server.serve_forever()
