#!/usr/bin/env python3
"""Write a realistic demo sessions.json for screenshots and manual testing.

Usage: python3 scripts/demo_data.py DATA_DIR [solves]

Creates DATA_DIR/cubetimer/sessions.json with one 3x3 session: times that get
faster and level off, a few +2s and DNFs, two fumbled solves, and random-move
scrambles. Point the timer at it with XDG_DATA_HOME=DATA_DIR.
"""
import json
import math
import os
import random
import sys

random.seed(11)
data_dir = sys.argv[1]
count = int(sys.argv[2]) if len(sys.argv) > 2 else 300


def scramble():
    out, faces = [], []
    while len(out) < 20:
        f = random.randrange(6)
        if faces and (f == faces[-1] or (len(faces) > 1 and f % 3 == faces[-1] % 3 == faces[-2] % 3)):
            continue
        faces.append(f)
        out.append("URFDLB"[f] + random.choice(["", "'", "2"]))
    return " ".join(out)


solves = []
for i in range(count):
    t = 16.0 - 3.4 * (1 - math.exp(-i / 120)) + random.gauss(0, 1.0)
    t += 1.8 if random.random() < 0.06 else 0
    t = max(t, 8.5)
    penalty = "ok"
    if i in (40, 121, 205, 260):
        penalty = "dnf"
    elif random.random() < 0.025:
        penalty = "plus2"
    if i in (88, 190):
        t = random.choice([61.2, 74.9])
    solves.append({"ms": int(t * 1000), "penalty": penalty, "scramble": scramble(), "ts": 1792000000 + i * 90})

os.makedirs(os.path.join(data_dir, "cubetimer"), exist_ok=True)
with open(os.path.join(data_dir, "cubetimer", "sessions.json"), "w") as f:
    json.dump({"sessions": [{"solves": solves, "puzzle": "333"}], "current": 0}, f)
print(f"wrote {count} solves to {data_dir}/cubetimer/sessions.json")
