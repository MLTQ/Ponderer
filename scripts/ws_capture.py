"""Small stdlib-only, loopback WebSocket event observer for isolated regressions."""
import base64
import hashlib
import json
import os
import socket
import struct
import threading
from urllib.parse import urlparse


class EventCapture:
    def __init__(self, url, token):
        endpoint = urlparse(url)
        if endpoint.hostname != "127.0.0.1":
            raise ValueError("This regression observer is loopback-only")
        self.events = []
        self.error = None
        self.closing = False
        self.lock = threading.Lock()
        self.socket = socket.create_connection((endpoint.hostname, endpoint.port), timeout=5)
        self.socket.settimeout(60)
        key = base64.b64encode(os.urandom(16)).decode()
        self.socket.sendall((f"GET {endpoint.path} HTTP/1.1\r\nHost: {endpoint.netloc}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\nAuthorization: Bearer {token}\r\n\r\n").encode())
        header = b""
        while not header.endswith(b"\r\n\r\n"):
            header += self._read(1)
            if len(header) > 16384:
                raise ValueError("Oversized WebSocket handshake")
        expected = base64.b64encode(hashlib.sha1((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()).digest())
        if b" 101 " not in header.split(b"\r\n", 1)[0] or expected not in header:
            raise ValueError("Invalid WebSocket upgrade")
        self.thread = threading.Thread(target=self._receive, daemon=True)
        self.thread.start()

    def _read(self, count):
        result = b""
        while len(result) < count:
            chunk = self.socket.recv(count - len(result))
            if not chunk:
                raise EOFError("Event socket closed")
            result += chunk
        return result

    def _receive(self):
        message = b""
        try:
            while not self.closing:
                first, second = self._read(2)
                opcode = first & 15
                length = second & 127
                if second & 128:
                    raise ValueError("Server frames must not be masked")
                if length == 126:
                    length = struct.unpack("!H", self._read(2))[0]
                elif length == 127:
                    length = struct.unpack("!Q", self._read(8))[0]
                if length > 8 * 1024 * 1024 or len(message) + length > 8 * 1024 * 1024:
                    raise ValueError("Oversized event frame")
                data = self._read(length)
                if opcode == 8:
                    return
                if opcode == 9:
                    mask = os.urandom(4)
                    self.socket.sendall(bytes((0x8A, 0x80 | len(data))) + mask + bytes(byte ^ mask[i % 4] for i, byte in enumerate(data)))
                    continue
                if opcode not in (0, 1):
                    continue
                message += data
                if first & 128:
                    event = json.loads(message.decode())
                    with self.lock:
                        self.events.append(event)
                    message = b""
        except (OSError, EOFError, ValueError) as error:
            if not self.closing:
                self.error = str(error)

    def snapshot(self):
        with self.lock:
            return list(self.events)

    def close(self):
        self.closing = True
        try:
            self.socket.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.socket.close()
        self.thread.join(timeout=2)
