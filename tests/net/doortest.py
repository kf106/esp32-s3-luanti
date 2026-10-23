import sys, time, struct, os, math
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "doorman.inv"), "w").write("kit 1\n0 esp:door_wood 2\n1 esp:door_steel 1\n2 esp:birch_wood 6\n3 esp:door_glass 1\n4 esp:trapdoor 1\n6 esp:trapdoor_steel 1\n")
def lists(c):
    text = c[6:6 + struct.unpack_from(">I", c, 2)[0]].decode()
    out = {}
    for block in text.split("EndInventoryList\n")[:-1]:
        head, *rest = block.split("\n")
        out[head.split()[1]] = [l[5:] if l.startswith("Item ") else None for l in rest[1:] if l]
    return out
def interact(cl, action, item, under, above, pitch=30.0, yaw=-90.0):
    pointed = struct.pack(">BB3h3h", 0, 1, *under, *above)
    p = [int(v * 100) for v in cl.pos_bs]
    cl.send(0, struct.pack(">HBH", 0x39, action, item) + struct.pack(">I", len(pointed)) + pointed
            + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, int(pitch * 100), int(yaw * 100), 0, 72, 5))
at = lambda pos: (lambda m: struct.unpack_from(">hhh", m, 2) == pos)
def nodes(cl, *wanted, cmd=0x21, secs=5):
    got = {}
    end = time.time() + secs
    while time.time() < end and len(got) < len(wanted):
        for m in cl.take(cmd):
            p = struct.unpack_from(">hhh", m, 2)
            if p in wanted: got[p] = m
        cl.pump(0.05)
    assert len(got) == len(wanted), f"got {list(got)} of {wanted}"
    return [got[w] for w in wanted]
c = Client("Doorman", port); c.login(); c.playerpos(); c.pump(0.3)
lists(c.wait_for(0x27))
for i in (0, 1, 3, 4, 6, 7):
    c.cmds.clear(); c.inventory_action(f"Move 1 current_player main 2 current_player craft {i}"); inv = lists(c.wait_for(0x27))
print("six planks:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:door_wood 1"]
for i in (0, 1, 3, 4, 6, 7):
    c.cmds.clear(); c.inventory_action(f"Move 1 current_player craft {i} current_player main 2"); c.wait_for(0x27)
for i in (3, 4, 5, 6, 7, 8):
    c.cmds.clear(); c.inventory_action(f"Move 1 current_player main 2 current_player craft {i}"); inv = lists(c.wait_for(0x27))
print("two rows of planks:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:trapdoor 2"]
for i in (3, 4, 5, 6, 7, 8):
    c.cmds.clear(); c.inventory_action(f"Move 1 current_player craft {i} current_player main 2"); c.wait_for(0x27)
px, py, pz = (v / 10 for v in c.pos_bs)
gx, gz, gy = round(px), round(pz), round(py - 0.6)
spot = (gx + 2, gy + 1, gz + 2); top = (gx + 2, gy + 2, gz + 2)
end = time.time() + 15
while time.time() < end and not any(struct.unpack_from(">hhh", m, 2) == tuple(v // 16 for v in spot) for m in c.take(0x20)):
    c.pump(0.1)
# Looking east (+X): facedir 1.
c.cmds.clear(); interact(c, 3, 0, (spot[0], gy, spot[2]), spot)
a, h = nodes(c, spot, top)
print("door:", u16(a, 8), a[11], "top:", u16(h, 8), h[11]); assert (u16(a, 8), a[11], u16(h, 8), h[11]) == (81, 1, 93, 1)
snd = c.wait_for(0x3f); print("place sound:", snd[8:8 + u16(snd, 6)].decode())
# Beside it (facedir 1 looks for a door at z + 1... from the new door's side, z - 1): a pair, hinged the other way.
spot2 = (gx + 2, gy + 1, gz + 1)
c.cmds.clear(); interact(c, 3, 0, (spot2[0], gy, spot2[2]), spot2)
b, h2 = nodes(c, spot2, (spot2[0], spot2[1] + 1, spot2[2]))
print("second door:", u16(b, 8), b[11], "top:", u16(h2, 8), h2[11]); assert (u16(b, 8), b[11], u16(h2, 8), h2[11]) == (82, 1, 93, 0)
# Open, close.
c.cmds.clear(); interact(c, 3, 2, spot, (spot[0] - 1, spot[1], spot[2]))
o = c.wait_for(0x21, 5, cond=at(spot)); s = c.wait_for(0x3f)
print("opened:", u16(o, 8), o[11], s[8:8 + u16(s, 6)].decode()); assert (u16(o, 8), o[11]) == (83, 2)
c.cmds.clear(); interact(c, 3, 2, spot, (spot[0] - 1, spot[1], spot[2]))
o = c.wait_for(0x21, 5, cond=at(spot)); s = c.wait_for(0x3f)
print("closed:", u16(o, 8), o[11], s[8:8 + u16(s, 6)].decode()); assert (u16(o, 8), o[11]) == (81, 1)
# Nothing can be built into the top half.
c.cmds.clear(); interact(c, 3, 2, (top[0], top[1] + 1, top[2]), top); c.pump(1.0)
assert not any(at(top)(m) and u16(m, 8) != 93 for m in c.take(0x21)), "built into a door's top"
# A steel door, which the thief can't open.
spot3 = (gx + 2, gy + 1, gz + 4)
c.cmds.clear(); interact(c, 3, 1, (spot3[0], gy, spot3[2]), spot3)
st = c.wait_for(0x21, 5, cond=at(spot3)); print("steel door:", u16(st, 8)); assert u16(st, 8) == 85
t = Client("Thief", port); t.login(); t.pos_bs = c.pos_bs; t.playerpos(); t.pump(0.5); t.cmds.clear()
interact(t, 3, 0, spot3, (spot3[0] - 1, spot3[1], spot3[2]))
msg = t.wait_for(0x2F, 5); text = msg.decode("utf-16-be", errors="replace")
print("thief told it's locked:", "locked" in text); assert "locked" in text
t.pump(0.5); assert not any(at(spot3)(m) for m in t.take(0x21))
t.cmds.clear(); interact(t, 2, 0, spot3, (spot3[0] - 1, spot3[1], spot3[2])); t.pump(1.0); assert not t.take(0x22), "thief dug it"
saved = open(os.path.join(os.environ["TEST_WORLD"], "world/doors.txt")).read(); print("doors.txt:", saved.strip())
assert "Doorman" in saved
# Trapdoors: placed facing away from the player (north here: 0), opened in place.
tspot = (gx + 2, gy + 1, gz + 6)
c.cmds.clear(); interact(c, 3, 4, (tspot[0], gy, tspot[2]), tspot)
tr = c.wait_for(0x21, 5, cond=at(tspot)); print("trapdoor:", u16(tr, 8), tr[11]); assert (u16(tr, 8), tr[11]) == (94, 0)
c.cmds.clear(); interact(c, 3, 2, tspot, (tspot[0], tspot[1] + 1, tspot[2]))
tr = c.wait_for(0x21, 5, cond=at(tspot)); s = c.wait_for(0x3f)
print("trapdoor opened:", u16(tr, 8), tr[11], s[8:8 + u16(s, 6)].decode()); assert (u16(tr, 8), tr[11]) == (95, 0)
sspot = (gx + 2, gy + 1, gz + 5)
c.cmds.clear(); interact(c, 3, 6, (sspot[0], gy, sspot[2]), sspot)
tr = c.wait_for(0x21, 5, cond=at(sspot)); assert u16(tr, 8) == 96
t.cmds.clear(); interact(t, 3, 0, sspot, (sspot[0], sspot[1] + 1, sspot[2]))
msg = t.wait_for(0x2F, 5); assert "locked" in msg.decode("utf-16-be", errors="replace")
t.pump(0.5); assert not any(at(sspot)(m) and u16(m, 8) == 97 for m in t.take(0x21)); print("steel trapdoor locked to the thief")
# Dig the wooden door by hand: both nodes go, the door comes back.
c.cmds.clear(); interact(c, 2, 5, spot, (spot[0] - 1, spot[1], spot[2]))
nodes(c, spot, top, cmd=0x22)
inv = lists(c.wait_for(0x27, cond=lambda m: b"esp:door_wood 1" in m)); print("dug it:", [x for x in inv["main"] if x])
for cl in (c, t): cl.raw(0, b"\x00\x03")
time.sleep(0.3); print("DOORS OK")
