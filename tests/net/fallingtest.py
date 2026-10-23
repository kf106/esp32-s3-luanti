"""Falling sand over the network: python3 fallingtest.py PORT netclient.py."""
import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "sandman.inv"), "w").write("kit 1\n0 esp:sand 6\n")
import zstandard
def interact(cl, action, item, under, above):
    pointed = struct.pack(">BB3h3h", 0, 1, *under, *above)
    p = [int(v * 100) for v in cl.pos_bs]
    cl.send(0, struct.pack(">HBH", 0x39, action, item) + struct.pack(">I", len(pointed)) + pointed
            + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, 3000, -9000, 0, 72, 5))
c = Client("Sandman", port); c.login(); c.playerpos(); c.pump(0.3)
state = {}
end = time.time() + 6
while time.time() < end:
    for m in c.take(0x20):
        bx, by, bz = struct.unpack_from(">hhh", m, 2)
        raw = zstandard.ZstdDecompressor().decompressobj().decompress(m[8:])
        for i in range(4096):
            z, y, x = i // 256, (i // 16) % 16, i % 16
            state[(bx * 16 + x, by * 16 + y, bz * 16 + z)] = struct.unpack_from(">H", raw, 5 + 2 * i)[0]
    c.pump(0.1)
px, py, pz = (v / 10 for v in c.pos_bs)
x, z = round(px) + 2, round(pz)
gy = max(y for (sx, y, sz), v in state.items() if (sx, sz) == (x, z) and v in (0, 1, 2, 3, 4, 5))
def track(secs):
    seen = {}
    end = time.time() + secs
    while time.time() < end:
        for m in c.cmds:
            cmd = u16(m)
            if cmd == 0x21: seen[struct.unpack_from(">hhh", m, 2)] = u16(m, 8)
            elif cmd == 0x22: seen[struct.unpack_from(">hhh", m, 2)] = 126
        c.cmds = [m for m in c.cmds if u16(m) not in (0x21, 0x22)]
        c.pump(0.03)
    return seen
# A column of three sand on the ground.
col = [(x, gy + i, z) for i in (1, 2, 3)]
under = (x, gy, z)
for p in col:
    c.cmds.clear(); interact(c, 3, 0, under, p); print("placed", p, track(0.4)); under = p
# Dig the bottom one: the two above fall a node.
c.cmds.clear(); interact(c, 2, 1, col[0], col[1])
seen = track(2.0)
print("after digging the bottom sand:", {p: seen.get(p) for p in col})
assert seen.get(col[0]) == 3 and seen.get(col[1]) == 3 and seen.get(col[2]) == 126, seen
# Sand put against the side of the column, three nodes up, falls to the ground.
side = (x + 1, gy + 2, z)
c.cmds.clear(); interact(c, 3, 0, col[1], side)
seen = track(2.0)
print("sand put in the air:", {p: seen.get(p) for p in [side, (x + 1, gy + 1, z)]})
assert seen.get((x + 1, gy + 1, z)) == 3 and seen.get(side) == 126, seen
c.raw(0, b"\x00\x03"); time.sleep(0.3); print("FALLING OK")
