"""Bottles, glasses and the vessels shelf over the network, on a fresh
world: python3 vesselstest.py PORT netclient.py"""
import sys, time, struct, os, zlib
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
exec(src[src.index("def formspec(c)"):src.index('c = Client("Skinner"')])
port = int(sys.argv[1])
GLASS_BOTTLE, DRINKING_GLASS, STEEL_BOTTLE, VESSELS_SHELF = 389, 390, 391, 392
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "potter.inv"), "w").write(
    "kit 1\n0 esp:glass 7\n1 esp:shovel_steel 1\n2 esp:glass_bottle 2\n3 esp:vessels_shelf 1\n4 esp:book 1\n5 esp:steel_bottle 1\n6 esp:drinking_glass 2\n")

def lists(c):
    text = c[6:6 + struct.unpack_from(">I", c, 2)[0]].decode()
    out = {}
    for block in text.split("EndInventoryList\n")[:-1]:
        head, *rest = block.split("\n")
        out[head.split()[1]] = [l[5:] if l.startswith("Item ") else None for l in rest[1:] if l]
    return out
def dlists(c):
    n = u16(c, 2); text = c[2 + 2 + n + 1 + 2:].decode()
    out = {}
    for block in text.split("EndInventoryList\n")[:-1]:
        head, *rest = block.split("\n")
        out[head.split()[1]] = [l[5:] if l.startswith("Item ") else None for l in rest[1:] if l]
    return out
def act(cl, text, want=0x27):
    cl.cmds.clear(); cl.inventory_action(text); return cl.wait_for(want)
def interact(cl, action, item, under, above):
    pointed = struct.pack(">BB3h3h", 0, 1, *under, *above)
    p = [int(v * 100) for v in cl.pos_bs]
    cl.send(0, struct.pack(">HBH", 0x39, action, item) + struct.pack(">I", len(pointed)) + pointed
            + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, 3000, -9000, 0, 72, 5))
def infotext(m):
    n = struct.unpack_from(">I", m, 2)[0]; b = zlib.decompress(m[6:6 + n])
    o = 1 + 2 + 6; nvars = struct.unpack_from(">I", b, o)[0]; o += 4; out = {}
    for _ in range(nvars):
        k = u16(b, o); name = b[o + 2:o + 2 + k].decode(); o += 2 + k
        v = struct.unpack_from(">I", b, o)[0]; out[name] = b[o + 4:o + 4 + v].decode(); o += 4 + v + 1
    return out.get("infotext")
def added_at(cl, pos):
    return cl.wait_for(0x21, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == pos)

c = Client("Potter", port); c.login(); c.playerpos(); c.pump(0.3)
lists(c.wait_for(0x27))
# Seven glass as a cup make 14 drinking glasses; as a bottle, 10 bottles.
for i in (0, 2, 3, 5, 6, 7, 8):
    inv = lists(act(c, f"Move 1 current_player main 0 current_player craft {i}"))
print("seven glass:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:drinking_glass 14"]
for i in (6, 8):
    inv = lists(act(c, f"Move 1 current_player craft {i} current_player main 0"))
print("five glass:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:glass_bottle 10"]
for i in (0, 2, 3, 5, 7):
    act(c, f"Move 1 current_player craft {i} current_player main 0")
# Two drinking glasses break into glass fragments.
for i in (0, 4):
    inv = lists(act(c, f"Move 1 current_player main 6 current_player craft {i}"))
print("two glasses:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:glass_fragments 1"]
for i in (0, 4):
    act(c, f"Move 1 current_player craft {i} current_player main 6")

# A bottle stands on the ground, and falls off (and drops) when it goes.
px, py, pz = (v / 10 for v in c.pos_bs)
gx, gz, gy = round(px), round(pz), round(py - 0.6)
spot = (gx + 2, gy + 1, gz)
ground = (spot[0], gy, spot[2])
c.cmds.clear(); interact(c, 3, 2, ground, spot)
m = added_at(c, spot); print("placed", u16(m, 8)); assert u16(m, 8) == GLASS_BOTTLE
# Nothing solid to stand on: not placed (on top of the bottle).
side = (spot[0], spot[1] + 1, spot[2])
c.cmds.clear(); interact(c, 3, 2, spot, side)
c.pump(1.0); assert not any(struct.unpack_from(">hhh", m, 2) == side and u16(m, 8) == GLASS_BOTTLE for m in c.take(0x21)), "a bottle in the air"
c.cmds.clear(); interact(c, 2, 1, ground, spot)
gone = set(); end = time.time() + 5
while time.time() < end and not {ground, spot} <= gone:
    c.pump(0.1); gone |= {struct.unpack_from(">hhh", m, 2) for m in c.take(0x22)}
assert {ground, spot} <= gone, gone
print("ground dug, the bottle fell off")
# (The dropped bottle lies as an item where it stood.)
c.pump(0.5)

# The vessels shelf: placed, empty, its window with bottle outlines.
spot2 = (gx - 2, gy + 1, gz); up = (spot2[0], spot2[1] + 1, spot2[2])
c.cmds.clear(); interact(c, 3, 3, (spot2[0], gy, spot2[2]), spot2)
m = added_at(c, spot2); print("placed", u16(m, 8)); assert u16(m, 8) == VESSELS_SHELF
it = infotext(c.wait_for(0x59)); print("pointed at:", it); assert it == "Empty Vessels Shelf"
c.cmds.clear(); interact(c, 3, 0, spot2, up)
det = c.wait_for(0x43); fs = formspec(c.wait_for(0x44))
loc = "detached:" + det[4:4 + u16(det, 2)].decode()
print("slots", len(dlists(det)["main"]), "outlines", fs.count("vessels_shelf_slot.png"))
assert len(dlists(det)["main"]) == 16 and fs.count("vessels_shelf_slot.png") == 16
# Vessels go on; a book doesn't.
c.cmds.clear(); c.inventory_action(f"Move 1 current_player main 5 {loc} main 3")
d = dlists(c.wait_for(0x43)); assert d["main"][3] == "esp:steel_bottle 1"
it = infotext(c.wait_for(0x59)); print("with a steel bottle:", it); assert it == "Vessels Shelf (1 items)"
c.cmds.clear(); c.inventory_action(f"Move 2 current_player main 6 {loc} main 4")
c.wait_for(0x43); it = infotext(c.wait_for(0x59)); fs = formspec(c.wait_for(0x44))
print("and two glasses:", it, "outlines", fs.count("vessels_shelf_slot.png"))
assert it == "Vessels Shelf (3 items)" and fs.count("vessels_shelf_slot.png") == 14
c.cmds.clear(); c.inventory_action(f"Move 1 current_player main 4 {loc} main 0")
d = dlists(c.wait_for(0x43)); print("book refused:", d["main"][0] is None); assert d["main"][0] is None
c.send(0, fields("esp:chest", "quit")); c.pump(0.3)
# Not dug while it holds anything.
c.cmds.clear(); interact(c, 2, 1, spot2, up); c.pump(1.0); assert not c.take(0x22), "dug a full shelf"
c.raw(0, b"\x00\x03"); time.sleep(0.3); print("VESSELS OK")
