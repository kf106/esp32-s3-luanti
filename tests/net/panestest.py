"""Glass panes and steel bars over the network: python3 panestest.py PORT netclient.py"""
import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
PANE_FLAT, PANE, BAR_DOOR, BAR_TRAPDOOR = 357, 358, 363, 367
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "glazier.inv"), "w").write(
    "kit 1\n0 esp:pane_flat 10\n1 esp:glass 6\n2 esp:door_steel_bar 1\n3 esp:trapdoor_steel_bar 1\n")

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
def changes(cl, secs=1.0):
    """The last ADDNODE/REMOVENODE seen at each position."""
    got = {}; end = time.time() + secs
    while time.time() < end:
        cl.pump(0.05)
        for m in cl.take(0x21): got[struct.unpack_from(">hhh", m, 2)] = (u16(m, 8), m[11])
        for m in cl.take(0x22): got[struct.unpack_from(">hhh", m, 2)] = ("air", 0)
    return got

c = Client("Glazier", port); c.login(); c.playerpos(); c.pump(0.3)
lists(c.wait_for(0x27))
# Six glass in two rows: 16 panes.
for i in range(6):
    c.cmds.clear(); c.inventory_action(f"Move 1 current_player main 1 current_player craft {i}"); inv = lists(c.wait_for(0x27))
print("six glass:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:pane_flat 16"]
c.cmds.clear(); c.inventory_action("Craft 1 current_player"); lists(c.wait_for(0x27))

px, py, pz = (v / 10 for v in c.pos_bs)
gx, gz, gy = round(px), round(pz), round(py - 0.6)
a, b, n = (gx + 2, gy + 1, gz), (gx + 3, gy + 1, gz), (gx + 2, gy + 1, gz + 1)
ground = lambda p: (p[0], p[1] - 1, p[2])
c.cmds.clear(); interact(c, 3, 0, ground(a), a); got = changes(c)
print("one pane:", got[a]); assert got[a][0] == PANE_FLAT
c.cmds.clear(); interact(c, 3, 0, ground(b), b); got = changes(c)
print("two in a row, east-west:", got); assert got[a] == (PANE_FLAT, 2) and got[b] == (PANE_FLAT, 0)
c.cmds.clear(); interact(c, 3, 0, ground(n), n); got = changes(c)
print("a corner:", got); assert got[a] == (PANE, 0) and got[n] == (PANE_FLAT, 3)
c.cmds.clear(); interact(c, 2, 5, b, (b[0], b[1] + 1, b[2])); got = changes(c)
print("east one dug:", got); assert got[b] == ("air", 0) and got[a] == (PANE_FLAT, 1)
inv = lists(c.wait_for(0x27)); print("inventory:", [x for x in inv["main"] if x]); assert any(x and x.startswith("esp:pane_flat") for x in inv["main"])
# The steel bar door and trapdoor go down like other steel ones.
d, t = (gx - 2, gy + 1, gz), (gx - 2, gy + 1, gz + 2)
c.cmds.clear(); interact(c, 3, 2, ground(d), d); got = changes(c)
print("bar door:", got.get(d)); assert got[d][0] == BAR_DOOR
s = 3
c.cmds.clear(); interact(c, 3, s, ground(t), t); got = changes(c)
print("bar trapdoor:", got.get(t)); assert got[t][0] == BAR_TRAPDOOR
c.cmds.clear(); interact(c, 3, s, t, t); got = changes(c)
print("opened:", got.get(t)); assert got[t][0] == BAR_TRAPDOOR + 1
c.raw(0, b"\x00\x03"); time.sleep(0.3); print("PANES OK")
