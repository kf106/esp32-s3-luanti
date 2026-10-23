"""Paper, books and bookshelves over the network, on a fresh world:
python3 shelftest.py PORT netclient.py"""
import sys, time, struct, os, zlib
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
exec(src[src.index("def formspec(c)"):src.index('c = Client("Skinner"')])
port = int(sys.argv[1])
BOOKSHELF = 385
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "librarian.inv"), "w").write("kit 1\n0 esp:papyrus 9\n1 esp:axe_steel 1\n2 esp:book 2\n3 esp:bookshelf 1\n4 esp:paper 3\n")

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
    # one entry: version, count, pos, nvars, then name/value pairs
    o = 1 + 2 + 6; nvars = struct.unpack_from(">I", b, o)[0]; o += 4; out = {}
    for _ in range(nvars):
        k = u16(b, o); name = b[o + 2:o + 2 + k].decode(); o += 2 + k
        v = struct.unpack_from(">I", b, o)[0]; out[name] = b[o + 4:o + 4 + v].decode(); o += 4 + v + 1
    return out.get("infotext")

c = Client("Librarian", port); c.login(); c.playerpos(); c.pump(0.3)
lists(c.wait_for(0x27))
# Paper from three papyrus in a row; a book from three paper in a column.
for i in (3, 4, 5):
    inv = lists(act(c, f"Move 1 current_player main 0 current_player craft {i}"))
print("three papyrus:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:paper 1"]
for i in (3, 4, 5):
    act(c, f"Move 1 current_player craft {i} current_player main 0")
for i in (1, 4, 7):
    inv = lists(act(c, f"Move 1 current_player main 4 current_player craft {i}"))
print("three paper:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:book 1"]
for i in (1, 4, 7):
    act(c, f"Move 1 current_player craft {i} current_player main 4")
# The bookshelf: placed facing us, "Empty Bookshelf" when pointed at.
px, py, pz = (v / 10 for v in c.pos_bs)
gx, gz, gy = round(px), round(pz), round(py - 0.6)
spot = (gx + 2, gy + 1, gz); up = (spot[0], spot[1] + 1, spot[2])
c.cmds.clear(); interact(c, 3, 3, (spot[0], gy, spot[2]), spot)
m = c.wait_for(0x21, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == spot)
print("placed", u16(m, 8)); assert u16(m, 8) == BOOKSHELF
it = infotext(c.wait_for(0x59)); print("pointed at:", it); assert it == "Empty Bookshelf"
# Opened: 16 slots, with book outlines in the empty ones.
c.cmds.clear(); interact(c, 3, 0, spot, up)
det = c.wait_for(0x43); fs = formspec(c.wait_for(0x44))
loc = "detached:" + det[4:4 + u16(det, 2)].decode()
print("slots", len(dlists(det)["main"]), "outlines", fs.count("default_bookshelf_slot.png"))
assert len(dlists(det)["main"]) == 16 and fs.count("default_bookshelf_slot.png") == 16
# Books go in; paper doesn't.
c.cmds.clear(); c.inventory_action(f"Move 2 current_player main 2 {loc} main 5")
d = dlists(c.wait_for(0x43)); assert d["main"][5] == "esp:book 2"
it = infotext(c.wait_for(0x59)); fs = formspec(c.wait_for(0x44))
print("with two books:", it, "outlines", fs.count("default_bookshelf_slot.png"))
assert it == "Bookshelf (0 written, 2 empty books)" and fs.count("default_bookshelf_slot.png") == 15
c.cmds.clear(); c.inventory_action(f"Move 1 current_player main 4 {loc} main 0")
d = dlists(c.wait_for(0x43)); print("paper refused:", d["main"][0] is None); assert d["main"][0] is None
# Not dug while it holds books.
c.cmds.clear(); interact(c, 2, 1, spot, up); c.pump(1.0); assert not c.take(0x22), "dug a full bookshelf"
c.cmds.clear(); c.inventory_action(f"MoveSomewhere 2 {loc} main 5 current_player main")
c.wait_for(0x43); it = infotext(c.wait_for(0x59)); print("emptied:", it); assert it == "Empty Bookshelf"
c.send(0, fields("esp:chest", "quit")); c.pump(0.3)
c.cmds.clear(); interact(c, 2, 1, spot, up); c.wait_for(0x22)
inv = lists(c.wait_for(0x27, cond=lambda m: b"esp:bookshelf 1" in m)); print("dug:", [x for x in inv["main"] if x])
c.raw(0, b"\x00\x03"); time.sleep(0.3); print("BOOKSHELVES OK")
