"""Keys, the screwdriver, binoculars and the mapping kit over the network,
on a fresh world: python3 keystest.py PORT netclient.py"""
import sys, time, struct, os, zlib
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
exec(src[src.index("def formspec(c)"):src.index('c = Client("Skinner"')])
port = int(sys.argv[1])
SNEAK = 1 << 6
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "alice.inv"), "w").write(
    "kit 1\n0 esp:chest_locked 1\n1 esp:trapdoor_steel 1\n2 esp:skeleton_key 3\n3 esp:screwdriver 1\n"
    "4 esp:chest 1\n5 esp:bed_bottom 1\n6 esp:mapping_kit 1\n9 esp:obsidian_glass 4\n10 esp:bronze_ingot 3\n")

def lists(c):
    text = c[6:6 + struct.unpack_from(">I", c, 2)[0]].decode()
    out = {}
    for block in text.split("EndInventoryList\n")[:-1]:
        head, *rest = block.split("\n")
        out[head.split()[1]] = [l[5:] if l.startswith("Item ") else None for l in rest[1:] if l]
    return out
def act(cl, text, want=0x27):
    cl.cmds.clear(); cl.inventory_action(text); return cl.wait_for(want)
def interact(cl, action, item, under=None, above=None, keys=0):
    pointed = struct.pack(">BB3h3h", 0, 1, *under, *above) if under else b"\x00\x00"
    p = [int(v * 100) for v in cl.pos_bs]
    cl.send(0, struct.pack(">HBH", 0x39, action, item) + struct.pack(">I", len(pointed)) + pointed
            + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, 3000, -9000, keys, 72, 5))
def chat(m):
    o = 4; n = u16(m, o); o += 2 + 2 * n; n = u16(m, o); o += 2
    return m[o:o + 2 * n].decode("utf-16-be")
def heard(cl, want, secs=3):
    lines = []; end = time.time() + secs
    while time.time() < end and not any(want in l for l in lines):
        cl.pump(0.05); lines += [chat(m) for m in cl.take(0x2F)]
    return lines
def added_at(cl, pos, secs=5):
    return cl.wait_for(0x21, secs, cond=lambda m: struct.unpack_from(">hhh", m, 2) == pos)
def none_at(cl, cmd, pos, secs=1.0):
    cl.pump(secs); return not any(struct.unpack_from(">hhh", m, 2) == pos for m in cl.take(cmd))
def minimap_flags(cl):
    m = cl.wait_for(0x4c, cond=lambda m: struct.unpack_from(">I", m, 6)[0] & 0x60)
    return struct.unpack_from(">II", m, 2)
ZOOM = struct.pack(">ff", 1.47, 10.0)

a = Client("Alice", port); a.login(); a.playerpos(); a.pump(0.3)
lists(a.wait_for(0x27))
# Alice carries a mapping kit: the minimap, but not its radar.
flags, mask = minimap_flags(a); print(f"Alice's HUD flags {flags:#x} of {mask:#x}"); assert (flags, mask) == (0x20, 0x60)
# ... shown as builtin's minimap HUD element (id 2, type 7).
a.wait_for(0x49, cond=lambda m: struct.unpack_from(">IB", m, 2) == (2, 7)); print("minimap HUD element added")

px, py, pz = (v / 10 for v in a.pos_bs)
gx, gz, gy = round(px), round(pz), round(py - 0.6)
chest = (gx + 2, gy + 1, gz)
trap = (gx - 2, gy + 1, gz)
plain = (gx, gy + 1, gz + 2)
def place(cl, slot, at):
    cl.cmds.clear(); interact(cl, 3, slot, (at[0], gy, at[2]), at)
    m = added_at(cl, at); return u16(m, 8), m[11]
LOCKED, _ = place(a, 0, chest)
TRAPDOOR, _ = place(a, 1, trap)
CHEST, facing = place(a, 4, plain)
print("placed", LOCKED, TRAPDOOR, CHEST)

# The skeleton key on her own chest and trapdoor: a key to each.
a.cmds.clear(); interact(a, 4, 2, chest, (chest[0], chest[1] + 1, chest[2]))
inv = lists(a.wait_for(0x27, cond=lambda m: b"Key to Alice's Locked Chest" in m))["main"]
print("skeleton keys left:", inv[2]); assert inv[2] == "esp:skeleton_key 2"
a.cmds.clear(); interact(a, 4, 2, trap, (trap[0], trap[1] + 1, trap[2]))
inv = lists(a.wait_for(0x27, cond=lambda m: b"Key to Alice's Steel Trapdoor" in m))["main"]
assert inv[2] == "esp:skeleton_key 1"
keys = [s for s in inv if s and s.startswith("esp:key ")]
print("keys:", len(keys)); assert len(keys) == 2

# The screwdriver: a left-click turns the chest about its face, a sneaking
# right-click about its axis; each turn wears it by 1/200.
a.cmds.clear(); interact(a, 4, 3, plain, (plain[0], plain[1] + 1, plain[2]))
m = added_at(a, plain); face = m[11]; print("facing", facing, "turned about its face to", face)
assert face == (facing + 1) % 4
a.cmds.clear(); interact(a, 3, 3, plain, (plain[0], plain[1] + 1, plain[2]), keys=SNEAK)
m = added_at(a, plain); axis = m[11]; print("turned about its axis to", axis); assert axis == 4
inv = lists(a.wait_for(0x27, cond=lambda m: b"esp:screwdriver 1 654" in m))["main"]
print("screwdriver:", inv[3]); assert inv[3] == "esp:screwdriver 1 654"
# Not someone else's chest, nor one with something in it: but Alice's own
# empty locked chest turns.
a.cmds.clear(); interact(a, 4, 3, chest, (chest[0], chest[1] + 1, chest[2]))
added_at(a, chest)

# A bed turns about its bottom half, taking its top with it.
DIRS = {0: (0, 1), 1: (1, 0), 2: (0, -1), 3: (-1, 0)}
for bed in ((gx, gy + 1, gz - 2), (gx - 3, gy + 1, gz - 3), (gx + 3, gy + 1, gz - 3), (gx, gy + 1, gz + 5)):
    a.cmds.clear(); interact(a, 3, 5, (bed[0], gy, bed[2]), bed)
    got = {}; end = time.time() + 2
    while time.time() < end and len(got) < 2:
        for m in a.take(0x21): got[struct.unpack_from(">hhh", m, 2)] = (u16(m, 8), m[11])
        a.pump(0.05)
    if bed in got: break
was = got[bed][1]; d = DIRS[was]; old_top = (bed[0] + d[0], bed[1], bed[2] + d[1])
print("bed:", got); assert old_top in got
a.cmds.clear(); interact(a, 4, 3, bed, (bed[0], bed[1] + 1, bed[2]))
new = (was + 1) % 4; d = DIRS[new]; new_top = (bed[0] + d[0], bed[1], bed[2] + d[1])
got = {}; gone = set(); end = time.time() + 5
while time.time() < end and (len(got) < 2 or old_top not in gone):
    for m in a.take(0x21): got[struct.unpack_from(">hhh", m, 2)] = (u16(m, 8), m[11])
    gone |= {struct.unpack_from(">hhh", m, 2) for m in a.take(0x22)}
    a.pump(0.05)
print("turned:", got, "removed", gone)
assert got[bed][1] == new and got[new_top][1] == new and old_top in gone

# Binoculars, crafted: used, they give a 10 degree zoom.
for i, slot in ((9, 0), (9, 2), (9, 6), (9, 8), (10, 3), (10, 4), (10, 5)):
    inv = lists(act(a, f"Move 1 current_player main {i} current_player craft {slot}"))
print("binoculars:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:binoculars 1"]
act(a, "Craft 1 current_player")
inv = lists(act(a, "Move 1 current_player craftresult 0 current_player main 7"))["main"]
print("main 7:", inv[7]); assert inv[7] == "esp:binoculars 1"
a.cmds.clear(); interact(a, 4, 7)
a.wait_for(0x32, cond=lambda m: ZOOM in m); print("zoom 10 with binoculars")
# Dropped: within 4.7 s (binoculars' cyclic_update), no zoom.
a.cmds.clear(); a.inventory_action("Drop 1 current_player main 7")
a.wait_for(0x32, 7, cond=lambda m: struct.pack(">ff", 1.47, 0.0) in m); print("no zoom without them")
# Without the mapping kit, within 5.3 s, no minimap.
a.cmds.clear(); a.inventory_action("Drop 1 current_player main 6")
a.wait_for(0x4a, 8, cond=lambda m: struct.unpack_from(">I", m, 2)[0] == 2)
assert struct.unpack_from(">II", a.wait_for(0x4c, 1, cond=lambda m: struct.unpack_from(">I", m, 6)[0] & 0x60), 2) == (0, 0x60)
print("minimap gone with the kit")
a.raw(0, b"\x00\x03"); time.sleep(0.5)

# Bob gets her keys.
saved = open(os.path.join(players, "alice.inv")).read().splitlines()
key_lines = [l.split(" ", 1)[1] for l in saved if " esp:key " in l]
chest_key = next(k for k in key_lines if "Locked Chest" in k)
trap_key = next(k for k in key_lines if "Steel Trapdoor" in k)
open(os.path.join(players, "bob.inv"), "w").write(f"kit 1\n0 {chest_key}\n1 {trap_key}\n2 esp:skeleton_key 1\n")
b = Client("Bob", port); b.login(); b.pos_bs = a.pos_bs; b.playerpos(); b.pump(0.5)
flags, mask = minimap_flags(b); print(f"Bob's HUD flags {flags:#x} of {mask:#x}"); assert (flags, mask) == (0, 0x60)
# Empty-handed, Alice's chest stays shut.
b.cmds.clear(); interact(b, 3, 5, chest, (chest[0], chest[1] + 1, chest[2]))
l = heard(b, "locked"); print(l); assert any("This chest is locked; it belongs to Alice." in x for x in l)
assert not b.take(0x43)
# The chest's key opens it; it doesn't open the trapdoor, whose key does.
b.cmds.clear(); interact(b, 3, 0, chest, (chest[0], chest[1] + 1, chest[2]))
b.wait_for(0x43); print("opened with the key")
b.send(0, fields("esp:chest", "quit")); b.pump(0.5)
b.cmds.clear(); interact(b, 3, 0, trap, (trap[0], trap[1] + 1, trap[2]))
assert none_at(b, 0x21, trap), "the chest's key opened the trapdoor"
b.cmds.clear(); interact(b, 3, 1, trap, (trap[0], trap[1] + 1, trap[2]))
m = added_at(b, trap); print("trapdoor now", u16(m, 8)); assert u16(m, 8) == TRAPDOOR + 1
# A skeleton key on someone else's chest: no.
b.cmds.clear(); interact(b, 4, 2, chest, (chest[0], chest[1] + 1, chest[2]))
l = heard(b, "own"); print(l); assert "You do not own this chest." in l
# The key digs it, as its owner could.
b.cmds.clear(); interact(b, 2, 0, chest, (chest[0], chest[1] + 1, chest[2]))
b.wait_for(0x22, cond=lambda m: struct.unpack_from(">hhh", m, 2) == chest); print("dug with the key")
b.raw(0, b"\x00\x03"); time.sleep(0.3); print("KEYS OK")
