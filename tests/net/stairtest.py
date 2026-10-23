import sys, time, struct, os, math
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "stairy.inv"), "w").write("kit 1\n0 esp:stair_cobble 10\n1 esp:slab_cobble 10\n2 esp:cobble 6\n5 esp:stair_inner_cobble 2\n6 esp:stair_outer_cobble 2\n3 esp:slab_birch_wood 2\n4 esp:pick_steel 1\n")
def lists(c):
    text = c[6:6 + struct.unpack_from(">I", c, 2)[0]].decode()
    out = {}
    for block in text.split("EndInventoryList\n")[:-1]:
        head, *rest = block.split("\n")
        out[head.split()[1]] = [l[5:] if l.startswith("Item ") else None for l in rest[1:] if l]
    return out
c = Client("Stairy", port); c.login(); c.playerpos(); c.pump(0.3)
def act(cl, text, want=0x27):
    cl.cmds.clear(); cl.inventory_action(text); return cl.wait_for(want)
def interact(cl, action, item, under, above, pitch=0.0, yaw=0.0):
    pointed = struct.pack(">BB3h3h", 0, 1, *under, *above)
    p = [int(v * 100) for v in cl.pos_bs]
    cl.send(0, struct.pack(">HBH", 0x39, action, item) + struct.pack(">I", len(pointed)) + pointed
            + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, int(pitch * 100), int(yaw * 100), 0, 72, 5))
lists(c.wait_for(0x27))
# Six cobblestone in a staircase: 8 stairs.
for i in (2, 4, 5, 6, 7, 8):
    inv = lists(act(c, f"Move 1 current_player main 2 current_player craft {i}"))
print("staircase:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:stair_cobble 8"]
for i in (2, 4, 5, 6, 7, 8):
    act(c, f"Move 1 current_player craft {i} current_player main 2")
px, py, pz = (v / 10 for v in c.pos_bs)
gx, gz = round(px), round(pz)
gy = round(py - 0.6)
ground = (gx + 2, gy, gz); spot = (gx + 2, gy + 1, gz)
end = time.time() + 15
while time.time() < end and not any(struct.unpack_from(">hhh", m, 2) == tuple(v // 16 for v in spot) for m in c.take(0x20)):
    c.pump(0.1)
def place(item, under, above, pitch=30.0, yaw=0.0):
    c.cmds.clear(); interact(c, 3, item, under, above, pitch, yaw)
    add = c.wait_for(0x21, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == above)
    return u16(add, 8), add[11]
# On the ground two to the east: faces away (+X, facedir 1), right way up.
node, p2 = place(0, ground, spot); print("stair on the ground:", node, p2); assert (node, p2) == (45, 1)
# Against the stair's west side, looking at its upper half: upside down
# (1 + 20 = 21, which the stairs mod swaps to 23).
side = (gx + 1, gy + 1, gz)
eye = py + 1.47
face_x = gx + 1.5 - px
pitch = -math.degrees(math.atan2(gy + 1.25 - eye, face_x))
node, p2 = place(0, spot, side, pitch, -90.0); print("upper half of a side:", node, p2); assert (node, p2) == (45, 23)
# A slab, and a slab on top of it keeps its turn; a wooden one on top of
# that is fine too.
s2 = (gx + 2, gy + 1, gz + 2)
node, p2 = place(1, (s2[0], gy, s2[2]), s2); print("slab:", node, p2); assert node == 55
up = (s2[0], s2[1] + 1, s2[2])
c.cmds.clear(); interact(c, 3, 1, s2, up, 30.0, 0.0)
add = c.wait_for(0x21, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == s2)
print("slab on a slab:", u16(add, 8)); assert u16(add, 8) == 17
inv = lists(c.wait_for(0x27, cond=lambda m: b"esp:slab_cobble 8" in m)); print("slabs left:", inv["main"][1])
# A wooden slab on the new block is just a slab.
node, _ = place(3, s2, up); assert node == 51
# And another wooden slab on that one makes birch planks.
c.cmds.clear(); interact(c, 3, 3, up, (up[0], up[1] + 1, up[2]), 30.0, 0.0)
add = c.wait_for(0x21, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == up); assert u16(add, 8) == 15
print("two birch slabs: planks")

# Dig one back.
c.cmds.clear(); interact(c, 2, 4, spot, (spot[0], spot[1] + 1, spot[2])); c.wait_for(0x22)
inv = lists(c.wait_for(0x27, cond=lambda m: b"esp:stair_cobble 9" in m)); print("after digging:", [x for x in inv["main"] if x])
assert "esp:stair_cobble 9" in inv["main"]
# Corners place like stairs: where the dug stair was, and on the planks.
node, p2 = place(5, ground, spot); print("inner corner:", node, p2); assert (node, p2) == (65, 1)
node, p2 = place(6, up, (up[0], up[1] + 1, up[2])); print("outer corner:", node, p2); assert node == 75
c.raw(0, b"\x00\x03"); time.sleep(0.3); print("STAIRS OK")
