"""The live probe must stop at EOF, including in fragmented WebSocket frames."""
import importlib.util
from pathlib import Path
import struct
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location("contract_check", Path(__file__).with_name("check.py"))
contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contract)
UPGRADE = b"HTTP/1.1 101 Switching Protocols\r\n\r\n"


class Peer:
    def __init__(self, chunks):
        self.chunks = iter(chunks)
        self.eof_reads = 0

    def settimeout(self, timeout):
        pass

    def connect(self, path):
        pass

    def sendall(self, data):
        pass

    def recv(self, size):
        chunk = next(self.chunks, b"")
        if not chunk:
            self.eof_reads += 1
            if self.eof_reads > 1:
                raise AssertionError("probe retried recv after EOF")
        return chunk


class LiveProbeTests(unittest.TestCase):
    def probe(self, chunks):
        peer = Peer(chunks)
        with patch.object(contract.socket, "socket", return_value=peer), \
                patch.object(contract, "check") as check, patch.object(contract, "warn"):
            contract.codex_live()
        return check

    def test_disconnect_during_upgrade_or_frame_stops_reading(self):
        cases = {
            "upgrade": [b"HTTP/1.1 101"],
            "frame_header": [UPGRADE],
            "partial_header": [UPGRADE, b"\x81"],
            "16_bit_length": [UPGRADE, b"\x81\x7e", b"\x00"],
            "64_bit_length": [UPGRADE, b"\x81\x7f", b"\x00" * 7],
            "payload": [UPGRADE, b"\x81\x10{}"],
            "close_frame": [UPGRADE, b"\x88\x00"],
        }
        for name, chunks in cases.items():
            with self.subTest(name=name), self.assertRaisesRegex(ConnectionError, "closed"):
                self.probe(chunks)

    def test_fragmented_upgrade_lengths_and_payloads(self):
        replies = [b'{"id":1,"result":{}}', b'{"id":2,"result":{"data":[]}}']
        for length_bytes in (0, 2, 8):
            chunks = [UPGRADE[:12], UPGRADE[12:]]
            for body in replies:
                header = b"\x81" + (
                    bytes([len(body)]) if length_bytes == 0 else
                    b"\x7e" + struct.pack(">H", len(body)) if length_bytes == 2 else
                    b"\x7f" + struct.pack(">Q", len(body))
                )
                chunks.extend(bytes([b]) for b in header + body)
            with self.subTest(length_bytes=length_bytes):
                check = self.probe(chunks)
                check.assert_any_call(True, "initialize answers")


if __name__ == "__main__":
    unittest.main()
