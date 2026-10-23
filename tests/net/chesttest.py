import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
exec(src[src.index("def formspec(c)"):src.index('c = Client("Skinner"')])
port = int(sys.argv[1])
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "chester.inv"), "w").write("kit 1\n0 esp:axe_steel 1\n1 esp:fir_wood 16\n2 esp:steel_ingot 1\n3 esp:cobble 10\n")
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
c = Client("Chester", port); c.login(); c.playerpos(); c.pump(0.3)
def act(cl, text, want=0x27):
    cl.cmds.clear(); cl.inventory_action(text); return cl.wait_for(want)
def interact(cl, action, item, under, above):
    pointed = struct.pack(">BB3h3h", 0, 1, *under, *above)
    p = [int(v * 100) for v in cl.pos_bs]
    cl.send(0, struct.pack(">HBH", 0x39, action, item) + struct.pack(">I", len(pointed)) + pointed + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, 0, 0, 0, 72, 5))
lists(c.wait_for(0x27))
for i in (0, 1, 2, 3, 5, 6, 7, 8):
    inv = lists(act(c, f"Move 1 current_player main 1 current_player craft {i}"))
print("ring of planks:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:chest 1"]
act(c, "Craft 1 current_player")
inv = lists(act(c, "Move 1 current_player craftresult 0 current_player main 4"))
# Chest + steel ingot anywhere: a locked chest (shapeless).
for i in (0, 1, 2, 3, 5, 6, 7, 8):
    act(c, f"Move 1 current_player main 1 current_player craft {i}")
act(c, "Craft 1 current_player")
act(c, "Move 1 current_player craftresult 0 current_player main 6")
act(c, "Move 1 current_player main 6 current_player craft 8")
inv = lists(act(c, "Move 1 current_player main 2 current_player craft 0"))
print("chest + ingot:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:chest_locked 1"]
act(c, "Craft 1 current_player")
inv = lists(act(c, "Move 1 current_player craftresult 0 current_player main 5"))
print("now has", [x for x in inv["main"] if x])
px, py, pz = (v / 10 for v in c.pos_bs)
ground = (round(px) + 2, round(py - 0.6), round(pz))
spot, spot2 = (ground[0], ground[1] + 1, ground[2]), (ground[0], ground[1] + 1, ground[2] + 2)
end = time.time() + 15
while time.time() < end and not any(struct.unpack_from(">hhh", m, 2) == tuple(v // 16 for v in spot) for m in c.take(0x20)):
    c.pump(0.1)
c.cmds.clear(); interact(c, 3, 4, ground, spot); add = c.wait_for(0x21)
print("placed chest id", u16(add, 8), "param2", add[11]); assert u16(add, 8) == 28
c.cmds.clear(); interact(c, 3, 5, (spot2[0], spot2[1] - 1, spot2[2]), spot2); add = c.wait_for(0x21); assert u16(add, 8) == 30
# Open: lid up, creak, window
up = lambda: (spot[0], spot[1] + 1, spot[2])
c.cmds.clear(); interact(c, 3, 0, spot, up())
lid = c.wait_for(0x21, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == spot)
snd = c.wait_for(0x3f); det = c.wait_for(0x43); fs = c.wait_for(0x44)
print("opened: node", u16(lid, 8), "sound", snd[8:8 + u16(snd, 6)].decode(), "slots", len(dlists(det)["main"]))
assert u16(lid, 8) == 29 and len(dlists(det)["main"]) == 32
loc = "detached:" + det[4:4 + u16(det, 2)].decode()
d = dlists(act(c, f"Move 10 current_player main 3 {loc} main 31", 0x43)); assert d["main"][31] == "esp:cobble 10"
# A full chest can't be dug; close it first (an open chest can't be dug either).
c.send(0, fields("esp:chest", "quit"))
shut = c.wait_for(0x21, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == spot and u16(m, 8) == 28); print("closed again, lid down")
c.cmds.clear(); interact(c, 2, 0, spot, up()); c.pump(1.0); assert not c.take(0x22), "dug a full chest"
# Someone else can't open the locked chest.
t = Client("Thief", port); t.login(); t.pos_bs = c.pos_bs; t.playerpos(); t.pump(0.5); t.cmds.clear()
pointed = struct.pack(">BB3h3h", 0, 1, *spot2, spot2[0], spot2[1] + 1, spot2[2])
p = [int(v * 100) for v in t.pos_bs]
t.send(0, struct.pack(">HBH", 0x39, 3, 0) + struct.pack(">I", len(pointed)) + pointed + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, 0, 0, 0, 72, 5))
msg = t.wait_for(0x2F, 5); text = msg.decode("utf-16-be", errors="replace")
print("thief told:", "locked" in text); assert "locked" in text and not t.take(0x44)
# Empty it and dig it.
interact(c, 3, 0, spot, up()); c.wait_for(0x44)
act(c, f"MoveSomewhere 10 {loc} main 31 current_player main", 0x43)
c.send(0, fields("esp:chest", "quit")); c.pump(0.5)
c.cmds.clear(); interact(c, 2, 0, spot, up()); c.wait_for(0x22)
inv = lists(c.wait_for(0x27, cond=lambda m: b"esp:chest 1" in m)); print("dug it back:", [x for x in inv["main"] if x])
saved = open(os.path.join(os.environ["TEST_WORLD"], "world/chests.txt")).read()
print("chests.txt:", saved.replace("\n", " | ")); assert "Chester" in saved
for cl in (c, t): cl.raw(0, b"\x00\x03")
time.sleep(0.5); print("CHESTS OK")
