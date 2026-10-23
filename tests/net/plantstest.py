"""Plants over the network: python3 plantstest.py PORT netclient.py KELP CORAL BERRIES,
each "x,y,z" from the harness's FIND_PLANTS."""
import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
kelp, coral, berries = (tuple(int(v) for v in a.split(",")) for a in sys.argv[3:6])
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "gardener.inv"), "w").write("kit 1\n0 esp:papyrus 3\n")

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
at = lambda pos: (lambda m: struct.unpack_from(">hhh", m, 2) == pos)
def node_at(cl, pos, secs=5):
    m = cl.wait_for(0x21, secs, cond=at(pos))
    return u16(m, 8), m[11]
def go(cl, pos):
    """Stand two nodes above a spot, and wait for its block."""
    cl.pos_bs = [pos[0] * 10, (pos[1] + 2) * 10, pos[2] * 10]; cl.playerpos()
    want = tuple(v // 16 for v in pos)
    end = time.time() + 20
    while time.time() < end and not any(struct.unpack_from(">hhh", m, 2) == want for m in cl.take(0x20)):
        cl.playerpos(); cl.pump(0.2)
def inventory_with(cl, text, secs=5):
    return lists(cl.wait_for(0x27, secs, cond=lambda m: text.encode() in m))["main"]

c = Client("Gardener", port); c.login(); c.playerpos(); c.pump(0.3)
lists(c.wait_for(0x27))
# Papyrus: three stacked on the ground; dig the bottom one and all three go.
px, py, pz = (v / 10 for v in c.pos_bs)
gx, gz, gy = round(px), round(pz), round(py - 0.6)
go(c, (gx + 2, gy, gz))
stack = [(gx + 2, gy + 1 + i, gz) for i in range(3)]
under = (gx + 2, gy, gz)
for p in stack:
    c.cmds.clear(); interact(c, 3, 0, under, p)
    assert node_at(c, p)[0] == 184, f"papyrus not placed at {p}"
    under = p
c.cmds.clear(); interact(c, 2, 1, stack[0], (stack[0][0], stack[0][1] + 1, stack[0][2]))
gone = set()
end = time.time() + 5
while time.time() < end and len(gone) < 3:
    for m in c.take(0x22): gone.add(struct.unpack_from(">hhh", m, 2))
    c.pump(0.05)
inv = inventory_with(c, "esp:papyrus 3")
print("dug the bottom papyrus: removed", sorted(gone), "inventory", [x for x in inv if x]); assert set(stack) <= gone
# Blueberries: pick them, the leaves stay; eat them.
go(c, berries)
c.cmds.clear(); interact(c, 2, 1, berries, (berries[0], berries[1] + 1, berries[2]))
got = node_at(c, berries); print("picked blueberries: the node is now", got); assert got[0] == 189
inv = inventory_with(c, "esp:blueberries"); print("inventory", [x for x in inv if x])
slot = next(i for i, x in enumerate(inv) if x and x.startswith("esp:blueberries"))
c.cmds.clear(); interact(c, 4, slot, berries, berries)
inv = lists(c.wait_for(0x27)); print("after eating:", [x for x in inv["main"] if x]); assert not any(x and "blueberries" in x for x in inv["main"])
# Kelp: dig it (the sand stays), put it back.
go(c, kelp)
c.cmds.clear(); interact(c, 2, 1, kelp, (kelp[0], kelp[1] + 1, kelp[2]))
got = node_at(c, kelp); print("dug kelp: the node is now", got); assert got[0] == 3
inv = inventory_with(c, "esp:sand_with_kelp")
slot = next(i for i, x in enumerate(inv) if x and x.startswith("esp:sand_with_kelp"))
# It grows 4 to 6 nodes tall, picked at random, and only goes in if
# that much water is above (as minetest_game's): in shallow water, try
# again.
for attempt in range(8):
    c.cmds.clear(); interact(c, 3, slot, kelp, (kelp[0], kelp[1] + 1, kelp[2]))
    try:
        got = node_at(c, kelp, 1.5); break
    except AssertionError:
        continue
print("put kelp back:", got, "height", got[1] // 16); assert got[0] == 203 and 4 <= got[1] // 16 <= 6
# A coral: dig it to its skeleton, put it back.
go(c, coral)
c.cmds.clear(); interact(c, 2, 1, coral, (coral[0], coral[1] + 1, coral[2]))
got = node_at(c, coral); print("dug coral: the node is now", got); assert got[0] == 209
inv = inventory_with(c, "esp:coral_")
slot = next(i for i, x in enumerate(inv) if x and x.startswith("esp:coral_"))
c.cmds.clear(); interact(c, 3, slot, coral, (coral[0], coral[1] + 1, coral[2]))
got = node_at(c, coral); print("put the coral back:", got); assert got[0] in (204, 205, 206)
c.raw(0, b"\x00\x03"); time.sleep(0.3); print("PLANTS OK")
