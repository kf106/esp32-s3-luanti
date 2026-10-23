"""Beds over the network: python3 bedtest.py PORT netclient.py, with the
harness serving a world whose clock starts at 22:00."""
import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
world = os.environ["TEST_WORLD"]
os.makedirs(world + "/players", exist_ok=True)
open(world + "/players/sleeper.inv", "w").write(
    "kit 1\n0 esp:bed_bottom 1\n1 esp:fancy_bed_bottom 1\n2 esp:wool_white 3\n3 esp:birch_wood 3\n")

def lists(c):
    text = c[6:6 + struct.unpack_from(">I", c, 2)[0]].decode()
    out = {}
    for block in text.split("EndInventoryList\n")[:-1]:
        head, *rest = block.split("\n")
        out[head.split()[1]] = [l[5:] if l.startswith("Item ") else None for l in rest[1:] if l]
    return out
def interact(cl, action, item, under, above, speed=(0, 0, 0), yaw=-90.0):
    pointed = struct.pack(">BB3h3h", 0, 1, *under, *above)
    p = [int(v * 100) for v in cl.pos_bs]
    cl.send(0, struct.pack(">HBH", 0x39, action, item) + struct.pack(">I", len(pointed)) + pointed
            + struct.pack(">3i3iiiIBB", *p, *speed, 3000, int(yaw * 100), 0, 72, 5))
def text_of(m): return m[2:].decode("utf-16-be", errors="replace")
def said(cl, words, secs=5):
    m = cl.wait_for(0x2F, secs, cond=lambda m: words.encode("utf-16-be") in m)
    return True
def form(cl, secs=5):
    m = cl.wait_for(0x44, secs)
    f = m[6:6 + struct.unpack_from(">I", m, 2)[0]].decode()
    name = m[6 + len(f) + 2:].decode()
    return f, name
def moved(cl, secs=5):
    m = cl.wait_for(0x34, secs)
    return [v / 10 for v in struct.unpack_from(">fff", m, 2)], struct.unpack_from(">ff", m, 14)
def fields_packet(formname, *names):
    body = struct.pack(">H", len(formname)) + formname.encode() + struct.pack(">H", len(names))
    for n in names: body += struct.pack(">H", len(n)) + n.encode() + struct.pack(">I", 1) + b"x"
    return struct.pack(">H", 0x3c) + body
at = lambda pos: (lambda m: struct.unpack_from(">hhh", m, 2) == pos)

c = Client("Sleeper", port); c.login(); c.playerpos(); c.pump(0.3)
lists(c.wait_for(0x27))
# Three wool on three planks: a simple bed.
for i, (slot, n) in enumerate([(2, 3), (2, 4), (2, 5), (3, 6), (3, 7), (3, 8)]):
    c.cmds.clear(); c.inventory_action(f"Move 1 current_player main {slot} current_player craft {n}"); inv = lists(c.wait_for(0x27))
print("wool on planks:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:bed_bottom 1"]
px, py, pz = (v / 10 for v in c.pos_bs)
gx, gz, gy = round(px), round(pz), round(py - 0.6)
spot = (gx + 2, gy + 1, gz + 2); top = (gx + 3, gy + 1, gz + 2)
end = time.time() + 15
while time.time() < end and not any(struct.unpack_from(">hhh", m, 2) == tuple(v // 16 for v in spot) for m in c.take(0x20)):
    c.pump(0.1)
# Looking east: the top goes east of the bottom, both turned 1.
c.cmds.clear(); interact(c, 3, 0, (spot[0], gy, spot[2]), spot)
got = {}
end = time.time() + 5
while time.time() < end and len(got) < 2:
    for m in c.take(0x21):
        got[struct.unpack_from(">hhh", m, 2)] = (u16(m, 8), m[11])
    c.pump(0.05)
print("bed:", got); assert got == {spot: (131, 1), top: (132, 1)}

o = Client("Owl", port); o.login(); o.pos_bs = c.pos_bs; o.playerpos(); o.pump(0.5)
# Moving: no.
c.cmds.clear(); interact(c, 3, 5, spot, (spot[0], spot[1] + 1, spot[2]), speed=(300, 0, 0))
said(c, "stop moving"); print("moving: told to stop")
c.cmds.clear(); interact(c, 3, 5, spot, (spot[0], spot[1] + 1, spot[2]))
pos, (pitch, yaw) = moved(c)
print("lies at", [round(v, 2) for v in pos], "yaw", yaw)
assert abs(pos[0] - (spot[0] + 0.5)) < 1e-3 and abs(pos[1] - (spot[1] + 0.07)) < 1e-3 and yaw == 90.0
f, name = form(c); print("window:", name, f)
assert name == "beds_form" and "1 of 2 players are in bed" in f and "force" not in f
assert c.wait_for(0x4c)[2:] == struct.pack(">II", 0, 8)
assert any(m[2:] == b"\x00\x00\x00\x00" * 8 + struct.pack(">f", 1.0) for m in c.take(0x51)) or c.wait_for(0x51)
# The other player sees them lie down.
lay = o.wait_for(0x32, cond=lambda m: struct.pack(">ff", 162.0, 166.0) in m); print("Owl sees the lay animation")
# Taken.
o.cmds.clear(); interact(o, 3, 0, spot, (spot[0], spot[1] + 1, spot[2]))
said(o, "already occupied"); print("Owl: occupied")
o.cmds.clear(); interact(o, 2, 0, spot, (spot[0], spot[1] + 1, spot[2])); o.pump(1.0)
assert not o.take(0x22), "dug an occupied bed"; print("Owl can't dig it")
# Leave Bed: back where they were.
c.cmds.clear(); c.send(0, fields_packet("beds_form", "leave", "quit"))
pos, _ = moved(c); print("got up at", [round(v, 2) for v in pos]); assert abs(pos[0] - px) < 1e-3
assert c.wait_for(0x4c)[2:] == struct.pack(">II", 8, 8)
# Die in bed: respawn at the bed.
c.cmds.clear(); interact(c, 3, 5, spot, (spot[0], spot[1] + 1, spot[2])); moved(c)
c.cmds.clear(); c.send(0, struct.pack(">HH", 0x35, 20)); c.wait_for(0x37)
# What they carried lies in bones beside the bed.
bones = struct.unpack_from(">hhh", c.wait_for(0x21, 5, cond=lambda m: u16(m, 8) == 369), 2)
c.cmds.clear(); c.send(0, struct.pack(">H", 0x38))
pos, _ = moved(c); print("respawned at", [round(v, 2) for v in pos])
assert [round(v, 2) for v in pos] == [spot[0], round(spot[1] + 0.07, 2), spot[2]]
c.pos_bs = [v * 10 for v in pos]
c.cmds.clear(); interact(c, 0, 0, bones, bones)
c.wait_for(0x22, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == bones); print("took back the bones at", bones)
# Bones give things back into the first free slots, so find the fancy bed.
inv = lists(c.wait_for(0x27, 5, cond=lambda m: b"esp:fancy_bed_bottom" in m))["main"]
fancy = next(i for i, x in enumerate(inv) if x and x.startswith("esp:fancy_bed_bottom"))
print("beds_spawns:", open(world + "/world/beds_spawns").read().strip())
# Into bed again; the owl leaves; everyone left is asleep: morning.
c.pos_bs = [v * 10 for v in pos]
c.cmds.clear(); interact(c, 3, 5, spot, (spot[0], spot[1] + 1, spot[2])); moved(c)
f, _ = form(c); assert "1 of 2" in f
o.raw(0, b"\x00\x03")
t0 = time.time()
f, _ = form(c, 8); print("window:", f); assert "Good morning." in f
tod = c.wait_for(0x29); print("time of day:", u16(tod, 2), "after", round(time.time() - t0, 1), "s"); assert u16(tod, 2) == 5520
moved(c)
# By day: only the respawn point.
c.cmds.clear(); interact(c, 3, 5, spot, (spot[0], spot[1] + 1, spot[2]))
said(c, "only sleep at night"); print("day: can only sleep at night")
fspot = (gx + 2, gy + 1, gz + 4); ftop = (gx + 2, gy + 1, gz + 5)
c.cmds.clear(); interact(c, 3, fancy, (fspot[0], gy, fspot[2]), fspot, yaw=0.0)
got = {}
end = time.time() + 5
while time.time() < end and len(got) < 2:
    for m in c.take(0x21):
        got[struct.unpack_from(">hhh", m, 2)] = (u16(m, 8), m[11])
    c.pump(0.05)
print("fancy bed:", got); assert got == {fspot: (133, 0), ftop: (134, 0)}
c.cmds.clear(); interact(c, 3, 5, fspot, (fspot[0], fspot[1] + 1, fspot[2]))
said(c, "Spawn point set"); print("spawn moved:", open(world + "/world/beds_spawns").read().strip())
# Dig it by hand: both halves go, the bed comes back, and the spawn goes.
c.cmds.clear(); interact(c, 2, 5, fspot, (fspot[0], fspot[1] + 1, fspot[2]))
gone = set()
end = time.time() + 5
while time.time() < end and len(gone) < 2:
    for m in c.take(0x22): gone.add(struct.unpack_from(">hhh", m, 2))
    c.pump(0.05)
assert gone == {fspot, ftop}, gone
inv = lists(c.wait_for(0x27, cond=lambda m: b"esp:fancy_bed_bottom" in m)); print("dug it:", [x for x in inv["main"] if x])
assert "Sleeper" not in open(world + "/world/beds_spawns").read()
c.raw(0, b"\x00\x03")
time.sleep(0.3); print("BEDS OK")
