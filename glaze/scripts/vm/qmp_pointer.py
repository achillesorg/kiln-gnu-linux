#!/usr/bin/env python3
"""Drive the VM's USB tablet through QMP `input-send-event` (host side).

Real pointer hardware events, so the compositor runs its normal pointer
paths (border resize, drags) exactly as for a user. Coordinates are output
pixels of a W x H screen (default 1280x800, scale 1).

Usage: qmp_pointer.py [--size WxH] CMD ... where CMD is one of
  move X Y | down [left|right|middle] | up [BUTTON] | sleep S
  drag X1 Y1 X2 Y2 [STEPS] | key COMBO (QMP qcodes joined by '-', e.g.
  meta_l-left, alt-tab, ctrl-alt-f1) | hold-drag X1 Y1 X2 Y2 SECS (drag that
  pauses at the end before releasing, to see a snap preview) | keydown Q | keyup Q
Environment: GLAZE_QMP_SOCK (default <repo>/../qmp.sock).
"""
import json
import os
import socket
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SOCK = os.environ.get("GLAZE_QMP_SOCK", os.path.join(ROOT, "..", "qmp.sock"))


class Qmp:
    def __init__(self, path):
        self.s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.s.settimeout(10)
        self.s.connect(path)
        self.f = self.s.makefile("rwb")
        self.f.readline()
        self.cmd("qmp_capabilities")

    def cmd(self, name, args=None):
        req = {"execute": name}
        if args:
            req["arguments"] = args
        self.f.write(json.dumps(req).encode() + b"\n")
        self.f.flush()
        while True:
            msg = json.loads(self.f.readline())
            if "return" in msg or "error" in msg:
                if "error" in msg:
                    raise RuntimeError(msg["error"])
                return msg


def main(argv):
    w, h = 1280, 800
    if len(argv) >= 2 and argv[0] == "--size":
        w, h = (int(v) for v in argv[1].split("x"))
        argv = argv[2:]
    q = Qmp(SOCK)

    def move(x, y):
        q.cmd("input-send-event", {"events": [
            {"type": "abs", "data": {"axis": "x", "value": int(x * 32767 / (w - 1))}},
            {"type": "abs", "data": {"axis": "y", "value": int(y * 32767 / (h - 1))}},
        ]})

    def btn(b, down):
        q.cmd("input-send-event", {"events": [
            {"type": "btn", "data": {"down": down, "button": b}}]})

    i = 0
    while i < len(argv):
        c = argv[i]
        if c == "move":
            move(float(argv[i + 1]), float(argv[i + 2])); i += 3
        elif c in ("down", "up"):
            b = argv[i + 1] if i + 1 < len(argv) and argv[i + 1] in ("left", "right", "middle") else "left"
            btn(b, c == "down"); i += 2 if b == (argv[i + 1] if i + 1 < len(argv) else None) else 1
        elif c == "sleep":
            time.sleep(float(argv[i + 1])); i += 2
        elif c == "drag":
            x1, y1, x2, y2 = (float(v) for v in argv[i + 1:i + 5])
            steps = 12
            i += 5
            if i < len(argv) and argv[i].isdigit():
                steps = int(argv[i]); i += 1
            move(x1 + 1, y1 + 1); time.sleep(0.03)
            move(x1, y1); time.sleep(0.08)
            btn("left", True); time.sleep(0.08)
            for k in range(1, steps + 1):
                move(x1 + (x2 - x1) * k / steps, y1 + (y2 - y1) * k / steps)
                time.sleep(0.02)
            time.sleep(0.05)
            btn("left", False)
        elif c == "key":
            keys = [{"type": "qcode", "data": k} for k in argv[i + 1].split("-")]
            q.cmd("send-key", {"keys": keys, "hold-time": 80}); i += 2
        elif c in ("keydown", "keyup"):
            q.cmd("input-send-event", {"events": [{"type": "key", "data": {
                "down": c == "keydown", "key": {"type": "qcode", "data": argv[i + 1]}}}]}); i += 2
        elif c == "hold-drag":
            x1, y1, x2, y2, hold = (float(v) for v in argv[i + 1:i + 6]); i += 6
            move(x1, y1); time.sleep(0.08)
            btn("left", True); time.sleep(0.08)
            for k in range(1, 21):
                move(x1 + (x2 - x1) * k / 20, y1 + (y2 - y1) * k / 20)
                time.sleep(0.03)
            time.sleep(hold)
            btn("left", False)
        else:
            raise SystemExit(f"unknown command {c}")


if __name__ == "__main__":
    main(sys.argv[1:])
