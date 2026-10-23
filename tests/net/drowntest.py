"""Drowning: python3 drowntest.py PORT netclient.py. The lake near (47, 47)
has its bed at y = -1."""
import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
c = Client("Diver", port); c.login(); c.pump(1)
c.pos_bs = [47 * 10, -0.5 * 10, 47 * 10]; c.playerpos(); c.cmds.clear()
breaths, hps, huds = [], [], []
end = time.time() + 24
while time.time() < end:
    c.playerpos(); c.pump(0.2)
    for m in c.take(0x4e): breaths.append(u16(m, 2))
    for m in c.take(0x33): hps.append(u16(m, 2))
    for m in c.take(0x49): huds.append(("add", struct.unpack_from(">I", m, 2)[0]))
print("underwater 24 s: breath", breaths, "HP", hps, "HUD", huds)
assert breaths[:3] == [9, 8, 7] and 0 in breaths and hps and hps[0] == 19 and ("add", 1) in huds
# Out of the water: breath comes back, two a second, and the bar goes.
c.pos_bs = [60 * 10, 2.5 * 10, 47 * 10]; c.playerpos(); c.cmds.clear()
breaths, removed = [], False
end = time.time() + 8
while time.time() < end and not removed:
    c.playerpos(); c.pump(0.2)
    for m in c.take(0x4e): breaths.append(u16(m, 2))
    removed = any(struct.unpack_from(">I", m, 2)[0] == 1 for m in c.take(0x4a))
print("in the air: breath", breaths, "bar removed:", removed)
assert breaths[-1] == 10 and removed
c.raw(0, b"\x00\x03"); time.sleep(0.3); print("DROWNING OK")
