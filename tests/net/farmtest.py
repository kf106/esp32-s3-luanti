import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "farmer.inv"), "w").write("kit 1\n0 esp:hoe_wood 1\n1 esp:seed_wheat 3\n2 esp:bucket_water 1\n3 esp:bread 2\n4 esp:seed_cotton 1\n8 esp:wheat 4\n9 esp:cotton 4\n")
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
c = Client("Farmer", port); c.login(); c.playerpos(); c.pump(0.3)
lists(c.wait_for(0x27))
# Flour from four wheat, anyhow; wool from four cotton.
for i, slot in ((0, 8), (2, 8), (4, 8), (8, 8)):
    c.cmds.clear(); c.inventory_action(f"Move 1 current_player main {slot} current_player craft {i}"); inv = lists(c.wait_for(0x27))
print("four wheat:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:flour 1"]
for i in (0, 2, 4, 8):
    c.cmds.clear(); c.inventory_action(f"Move 1 current_player craft {i} current_player main 8"); c.wait_for(0x27)
for i in (0, 1, 3, 4):
    c.cmds.clear(); c.inventory_action(f"Move 1 current_player main 9 current_player craft {i}"); inv = lists(c.wait_for(0x27))
print("four cotton:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:wool_white 1"]
for i in (0, 1, 3, 4):
    c.cmds.clear(); c.inventory_action(f"Move 1 current_player craft {i} current_player main 9"); c.wait_for(0x27)
px, py, pz = (v / 10 for v in c.pos_bs)
gx, gz, gy = round(px), round(pz), round(py - 0.6)
ground = (gx + 2, gy, gz + 2); top = (gx + 2, gy + 1, gz + 2)
end = time.time() + 15
while time.time() < end and not any(struct.unpack_from(">hhh", m, 2) == tuple(v // 16 for v in ground) for m in c.take(0x20)):
    c.pump(0.1)
# Clear any tall grass, then hoe.
interact(c, 2, 5, top, (top[0], top[1] + 1, top[2])); c.pump(0.5); c.cmds.clear()
interact(c, 4, 0, ground, top)
s = c.wait_for(0x21, 5, cond=at(ground)); print("hoed:", u16(s, 8)); assert u16(s, 8) == 106
snd = c.wait_for(0x3f); print("  sound", snd[8:8 + u16(snd, 6)].decode())
inv = lists(c.wait_for(0x27)); print("  hoe now", inv["main"][0]); assert inv["main"][0].startswith("esp:hoe_wood 1 ")
# Plant: a seed lying on the soil.
c.cmds.clear(); interact(c, 3, 1, ground, top)
s = c.wait_for(0x21, 5, cond=at(top)); print("planted:", u16(s, 8), "param2", s[11]); assert (u16(s, 8), s[11]) == (108, 1)
# It sprouts on dry soil, then waits for water.
s = c.wait_for(0x21, 10, cond=at(top)); print("sprouted:", u16(s, 8), "param2", s[11]); assert (u16(s, 8), s[11]) == (110, 3)
# Water beside it wets the soil, and it grows to ripe.
water = (gx + 3, gy + 1, gz + 2)
c.cmds.clear(); interact(c, 3, 2, (water[0], gy, water[2]), water); c.wait_for(0x21, 5, cond=at(water))
s = c.wait_for(0x21, 20, cond=lambda m: at(ground)(m) and u16(m, 8) == 107); print("soil wet")
stage = 110; end = time.time() + 30
while stage != 117 and time.time() < end:
    for m in c.take(0x21):
        if at(top)(m): stage = u16(m, 8)
    c.pump(0.1)
print("ripe:", stage); assert stage == 117
# Harvest: wheat and seeds.
c.cmds.clear(); interact(c, 2, 5, top, (top[0], top[1] + 1, top[2]))
c.wait_for(0x22, 5, cond=at(top))
inv = lists(c.wait_for(0x27, cond=lambda m: b"esp:wheat 5" in m or b"esp:wheat 6" in m)); print("harvested:", [x for x in inv["main"] if x and ("wheat" in x)])
# Eat bread.
c.cmds.clear(); interact(c, 4, 3, ground, top)
inv = lists(c.wait_for(0x27, cond=lambda m: b"esp:bread 1" in m)); print("ate bread")
# A seed falls off when its soil is dug.
c.cmds.clear(); interact(c, 3, 1, ground, top); c.wait_for(0x21, 5, cond=at(top))
c.cmds.clear(); interact(c, 2, 5, ground, top)
gone = []; end = time.time() + 5
while len(gone) < 2 and time.time() < end:
    gone += [struct.unpack_from(">hhh", m, 2) for m in c.take(0x22)]; c.pump(0.05)
print("dug the soil, removed:", gone); assert top in gone and ground in gone
c.raw(0, b"\x00\x03"); time.sleep(0.3); print("FARMING OK")
