#!/usr/bin/env python3
"""Capture the QEMU display through QMP `screendump` and write a PNG.

Works regardless of the compositor running in the guest, because QEMU reads
its own framebuffer. Usage: qmp_screendump.py QMP_SOCKET OUT.png

Tries `screendump format=png` first (QEMU >= 7.1 with libpng); otherwise dumps
PPM and converts it to PNG with the standard library only (zlib).
"""
import json
import os
import socket
import struct
import sys
import tempfile
import zlib


def qmp(sock_path, command, arguments=None):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(15)
    s.connect(sock_path)
    f = s.makefile("rwb")

    def read_reply():
        while True:
            line = f.readline()
            if not line:
                raise RuntimeError("QMP socket closed")
            msg = json.loads(line)
            if "return" in msg or "error" in msg:
                return msg

    f.readline()  # greeting
    f.write(b'{"execute":"qmp_capabilities"}\n')
    f.flush()
    read_reply()
    req = {"execute": command}
    if arguments:
        req["arguments"] = arguments
    f.write(json.dumps(req).encode() + b"\n")
    f.flush()
    reply = read_reply()
    s.close()
    return reply


def ppm_to_png(ppm_path, png_path):
    data = open(ppm_path, "rb").read()
    tokens, pos = [], 0
    while len(tokens) < 4:
        while data[pos:pos + 1].isspace():
            pos += 1
        if data[pos:pos + 1] == b"#":
            pos = data.index(b"\n", pos) + 1
            continue
        end = pos
        while not data[end:end + 1].isspace():
            end += 1
        tokens.append(data[pos:end])
        pos = end
    pos += 1  # single whitespace after maxval
    if tokens[0] != b"P6" or int(tokens[3]) != 255:
        raise RuntimeError("unsupported PPM: %r" % tokens)
    w, h = int(tokens[1]), int(tokens[2])
    pixels = data[pos:pos + w * h * 3]
    raw = b"".join(b"\x00" + pixels[y * w * 3:(y + 1) * w * 3] for y in range(h))

    def chunk(kind, body):
        c = struct.pack(">I", len(body)) + kind + body
        return c + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    open(png_path, "wb").write(png)


def main():
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    sock_path, out = sys.argv[1], os.path.abspath(sys.argv[2])
    reply = qmp(sock_path, "screendump", {"filename": out, "format": "png"})
    if "return" in reply and os.path.getsize(out) > 0:
        return 0
    fd, ppm = tempfile.mkstemp(suffix=".ppm")
    os.close(fd)
    try:
        reply = qmp(sock_path, "screendump", {"filename": ppm})
        if "error" in reply:
            print("screendump failed: %s" % reply["error"], file=sys.stderr)
            return 1
        ppm_to_png(ppm, out)
    finally:
        os.unlink(ppm)
    return 0


if __name__ == "__main__":
    sys.exit(main())
