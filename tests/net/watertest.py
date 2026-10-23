import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "waterer.inv"), "w").write("kit 1\n0 esp:bucket_water 1\n1 esp:cobble 5\n2 esp:bucket_empty 2\n3 esp:steel_ingot 3\n5 esp:torch 3\n")
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
            + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, 3000, 0, 0, 72, 5))
at = lambda pos: (lambda m: struct.unpack_from(">hhh", m, 2) == pos)
c = Client("Waterer", port); c.login(); c.playerpos(); c.pump(0.3)
lists(c.wait_for(0x27))
for i in (0, 2, 4):
    c.cmds.clear(); c.inventory_action(f"Move 1 current_player main 3 current_player craft {i}"); inv = lists(c.wait_for(0x27))
print("steel V:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:bucket_empty 1"]
for i in (0, 2, 4):
    c.cmds.clear(); c.inventory_action(f"Move 1 current_player craft {i} current_player main 3"); c.wait_for(0x27)
px, py, pz = (v / 10 for v in c.pos_bs)
gx, gz, gy = round(px), round(pz), round(py - 0.6)
spot = (gx + 2, gy + 1, gz + 2); spot2 = (gx + 2, gy + 1, gz + 3)
end = time.time() + 15
while time.time() < end and not any(struct.unpack_from(">hhh", m, 2) == tuple(v // 16 for v in spot) for m in c.take(0x20)):
    c.pump(0.1)
# Pour: a water source, and the bucket comes back empty.
c.cmds.clear(); interact(c, 3, 0, (spot[0], gy, spot[2]), spot)
w = c.wait_for(0x21, 5, cond=at(spot)); print("poured:", u16(w, 8)); assert u16(w, 8) == 98
inv = lists(c.wait_for(0x27, cond=lambda m: b"Item esp:bucket_empty 1" in m)); print("slot 0 now", inv["main"][0]); assert inv["main"][0] == "esp:bucket_empty 1"
# Scoop it up with the stack of two empty buckets: one fills and goes into
# the inventory.
c.cmds.clear(); interact(c, 4, 2, spot, (spot[0], spot[1] + 1, spot[2]))
r = c.wait_for(0x22, 5, cond=at(spot)); print("scooped up")
inv = lists(c.wait_for(0x27, cond=lambda m: b"esp:bucket_water" in m)); held = [x for x in inv["main"] if x and "bucket" in x]
print("buckets now:", held); assert "esp:bucket_empty 1" in held and "esp:bucket_water 1" in held
full = inv["main"].index("esp:bucket_water 1"); assert full < 8, "not in the hotbar"
# A torch can't go into water: the server puts the water back.
c.cmds.clear(); interact(c, 3, full, (spot2[0], gy, spot2[2]), spot2)
w = c.wait_for(0x21, 5, cond=at(spot2)); assert u16(w, 8) == 98
c.cmds.clear(); interact(c, 3, 5, (spot2[0], gy, spot2[2]), spot2)
w = c.wait_for(0x21, 5, cond=at(spot2)); print("torch into water: node stays", u16(w, 8)); assert u16(w, 8) == 98
# Build into the water: the cobblestone replaces it.
c.cmds.clear(); interact(c, 3, 1, (spot2[0], gy, spot2[2]), spot2)
w = c.wait_for(0x21, 5, cond=at(spot2)); print("built into water:", u16(w, 8)); assert u16(w, 8) == 17
# Scooping air does nothing.
c.cmds.clear(); interact(c, 4, 0, spot, (spot[0], spot[1] + 1, spot[2])); c.pump(0.8); assert not c.take(0x22)
c.raw(0, b"\x00\x03"); time.sleep(0.3); print("WATER OK")
