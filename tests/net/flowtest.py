"""Flowing water over the network: python3 flowtest.py PORT netclient.py.
The lake near (47, 47) has its east shore at (59, 1, 47) (SHORE=1)."""
import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
world = os.environ["TEST_WORLD"]
os.makedirs(world + "/players", exist_ok=True)
open(world + "/players/digger.inv", "w").write("kit 1\n0 esp:bucket_water 1\n1 esp:torch 2\n2 esp:shovel_steel 1\n")

def interact(cl, action, item, under, above):
    pointed = struct.pack(">BB3h3h", 0, 1, *under, *above)
    p = [int(v * 100) for v in cl.pos_bs]
    cl.send(0, struct.pack(">HBH", 0x39, action, item) + struct.pack(">I", len(pointed)) + pointed
            + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, 3000, 0, 0, 72, 5))
import zstandard
resent_blocks = []
def gather(cl, secs, seen=None):
    """Node changes seen in `secs`: pos -> (id, param2), air for removed.
    Blocks sent again count too: every node in them that is water, or was
    seen before."""
    seen = {} if seen is None else seen
    end = time.time() + secs
    while time.time() < end:
        for m in cl.cmds:
            cmd = u16(m)
            if cmd == 0x21:
                seen[struct.unpack_from(">hhh", m, 2)] = (u16(m, 8), m[11])
            elif cmd == 0x22:
                seen[struct.unpack_from(">hhh", m, 2)] = (126, 0)
            elif cmd == 0x20:
                bx, by, bz = struct.unpack_from(">hhh", m, 2)
                raw = zstandard.ZstdDecompressor().decompressobj().decompress(m[8:])
                resent_blocks.append((bx, by, bz))
                for i in range(4096):
                    z, y, x = i // 256, (i // 16) % 16, i % 16
                    pos = (bx * 16 + x, by * 16 + y, bz * 16 + z)
                    nid = struct.unpack_from(">H", raw, 5 + 2 * i)[0]
                    if nid in (98, 99) or pos in seen:
                        seen[pos] = (nid, raw[5 + 12288 + i])
        cl.cmds = [m for m in cl.cmds if u16(m) not in (0x20, 0x21, 0x22)]
        cl.pump(0.05)
    return seen
def wait_blocks(cl, *nodes):
    want = {tuple(v // 16 for v in n) for n in nodes}
    end = time.time() + 20
    while time.time() < end and want:
        for m in cl.take(0x20):
            want.discard(struct.unpack_from(">hhh", m, 2))
        cl.pump(0.1)
    assert not want, f"blocks {want} never came"

c = Client("Digger", port); c.login()
c.pos_bs = [61 * 10, 2.5 * 10, 47 * 10]; c.playerpos(); c.pump(0.3)
shore, beyond, below = (59, 1, 47), (60, 1, 47), (60, 0, 47)
wait_blocks(c, shore, beyond, below)
c.cmds.clear()
# Dig the shore: the lake flows in, full, and no further (sand beyond).
interact(c, 2, 2, shore, (shore[0], 2, shore[2]))
got = gather(c, 3.5)
print("shore dug:", got.get(shore)); assert got.get(shore) == (99, 7), got
# A pit beyond it: the water runs on and falls in.
interact(c, 2, 2, beyond, (beyond[0], 2, beyond[2]))
gather(c, 0.5)
interact(c, 2, 2, below, (below[0] + 1, 0, below[2]))
got = gather(c, 4)
print("pit:", {p: got[p] for p in (beyond, below) if p in got})
assert got.get(below, (0, 0))[0] == 99 and got[beyond][0] == 99 and got[beyond][1] & 8, got
# Pour a bucket on dry land with a torch nearby: the water spreads and
# washes the torch out.
land = (63, None, 47)
c.pos_bs = [66 * 10, 2.5 * 10, 47 * 10]; c.playerpos(); c.pump(0.5)
# The ground there: whatever was sent; find the top by digging nothing,
# place against the node below the player's feet.
gy = 3
spot = (66, gy + 1, 48); torch = (66, gy + 1, 50)
c.pump(1.0)  # already sent: the player has been near all along
c.cmds.clear()
interact(c, 3, 1, (torch[0], gy, torch[2]), torch)
t = gather(c, 1.0); print("torch:", t.get(torch))
interact(c, 3, 0, (spot[0], gy, spot[2]), spot)
got = gather(c, 4)
flows = sorted((p, v) for p, v in got.items() if v[0] == 99)
print("poured: source", got.get(spot), "and", len(flows), "flowing nodes, e.g.", flows[:3])
assert got.get(spot) == (98, 0) and len(flows) >= 8
assert got.get(torch, (0, 0))[0] == 99, f"torch still there: {got.get(torch)}"
items = [n for c_ in c.take(0x31) for _, n in remove_add(c_)[1]]
print("torch washed out, items added:", items); assert "__builtin:item" in items
# Scoop the source: the flowing water drains away.
c.cmds.clear(); interact(c, 4, 0, spot, (spot[0], spot[1] + 1, spot[2]))
got = gather(c, 15, dict(got))
left = [p for p, v in flows if got.get(p, v)[0] == 99]
print("after scooping, flowing left:", left); assert not left
print("blocks sent again:", len(resent_blocks))
c.raw(0, b"\x00\x03"); time.sleep(0.3); print("FLOW OK")
