"""Fences and gates over the network: python3 fencetest.py PORT netclient.py."""
import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "fencer.inv"), "w").write("kit 1\n0 esp:fir_wood 4\n1 esp:stick 2\n2 esp:fence_apple_wood 3\n3 esp:gate_birch_wood_closed 1\n")
def lists(c):
    text = c[6:6 + struct.unpack_from(">I", c, 2)[0]].decode()
    out = {}
    for block in text.split("EndInventoryList\n")[:-1]:
        head, *rest = block.split("\n")
        out[head.split()[1]] = [l[5:] if l.startswith("Item ") else None for l in rest[1:] if l]
    return out
def interact(cl, action, item, under, above):
    pointed = struct.pack(">BB3h3h", 0, 1, *under, *above)
    p = [int(v * 100) for v in cl.pos_bs]
    cl.send(0, struct.pack(">HBH", 0x39, action, item) + struct.pack(">I", len(pointed)) + pointed
            + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, 3000, -9000, 0, 72, 5))
def node_at(cl, pos, secs=5):
    m = cl.wait_for(0x21, secs, cond=lambda m: struct.unpack_from(">hhh", m, 2) == pos)
    return u16(m, 8), m[11]

c = Client("Fencer", port); c.login(); c.playerpos(); c.pump(0.3)
import zstandard
state = {}
end = time.time() + 6
while time.time() < end:
    for m in c.take(0x20):
        bx, by, bz = struct.unpack_from(">hhh", m, 2)
        raw = zstandard.ZstdDecompressor().decompressobj().decompress(m[8:])
        for i in range(4096):
            bz_, by_, bx_ = i // 256, (i // 16) % 16, i % 16
            state[(bx * 16 + bx_, by * 16 + by_, bz * 16 + bz_)] = struct.unpack_from(">H", raw, 5 + 2 * i)[0]
    c.pump(0.1)
lists(c.wait_for(0x27))
# Fir planks either side of sticks, two rows: four fir fences.
for slot, cell in [(0, 0), (1, 1), (0, 2), (0, 3), (1, 4), (0, 5)]:
    c.cmds.clear(); c.inventory_action(f"Move 1 current_player main {slot} current_player craft {cell}"); inv = lists(c.wait_for(0x27))
print("planks and sticks:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:fence_fir_wood 4"]
px, py, pz = (v / 10 for v in c.pos_bs)
gx, gz, gy = round(px), round(pz), round(py - 0.6)
def ground(x, z):
    """The top of the ground (grass, dirt, sand, stone) in a column."""
    return max(y for (sx, y, sz), v in state.items() if (sx, sz) == (x, z) and v in (0, 1, 2, 3, 4, 5))
# A row of fences east of the player, with a gate in it, on flat ground.
x = gx + 3
for z0 in range(gz - 6, gz + 6):
    if len({ground(x, z0 + d) for d in range(4)}) == 1:
        break
gy = ground(x, z0); gz = z0 + 1
row = [(x, gy + 1, gz + dz) for dz in (-1, 0, 2)]
for p in row:
    c.cmds.clear(); interact(c, 3, 2, (p[0], gy, p[2]), p)
    got = node_at(c, p); print("fence at", p, got); assert got == (138, 0)
gate = (x, gy + 1, gz + 1)
c.cmds.clear(); interact(c, 3, 3, (gate[0], gy, gate[2]), gate)
dx, dz = gate[0] - px, gate[2] - pz
facing = (1 if dx > 0 else 3) if abs(dx) > abs(dz) else (0 if dz > 0 else 2)
got = node_at(c, gate); print("gate:", got, "turned from the player:", facing); assert got == (145, facing)
# Open it, close it, with its sounds.
for want_id, sound in [(146, "doors_fencegate_open"), (145, "doors_fencegate_close")]:
    c.cmds.clear(); interact(c, 3, 5, gate, (gate[0] - 1, gate[1], gate[2]))
    got = node_at(c, gate)
    snd = c.wait_for(0x3f); name = snd[8:8 + u16(snd, 6)].decode()
    print("right-click:", got, name); assert got == (want_id, facing) and name == sound
# Open, then dig: the closed gate comes back.
c.cmds.clear(); interact(c, 3, 5, gate, (gate[0] - 1, gate[1], gate[2])); assert node_at(c, gate) == (146, facing)
c.cmds.clear(); interact(c, 2, 5, gate, (gate[0] - 1, gate[1], gate[2]))
c.wait_for(0x22, cond=lambda m: struct.unpack_from(">hhh", m, 2) == gate)
inv = lists(c.wait_for(0x27, cond=lambda m: b"esp:gate_birch_wood_closed" in m))
print("dug the open gate:", [x for x in inv["main"] if x]); assert "esp:gate_birch_wood_closed 1" in inv["main"]
c.raw(0, b"\x00\x03"); time.sleep(0.3); print("FENCES OK")
