"""A dungeon's loot chest, over the network, on a fresh world:
python3 dungeontest.py PORT netclient.py CHEST FRONT
(CHEST and FRONT are "x,y,z": a dungeon chest the harness found with
DUNGEONS=1, and the air in front of it.)"""
import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
exec(src[src.index("import zstandard"):src.index('g = Client("Digger"')])
port = int(sys.argv[1])
chest = tuple(int(v) for v in sys.argv[3].split(","))
front = tuple(int(v) for v in sys.argv[4].split(","))
CHEST, COBBLE, MOSSY, DESERT_STONE = 28, 17, 372, 227

def say(cl, text):
    u = text.encode("utf-16-be"); cl.send(0, struct.pack(">HH", 0x32, len(u) // 2) + u)
def dlists(c):
    n = u16(c, 2); text = c[2 + 2 + n + 1 + 2:].decode()
    out = {}
    for block in text.split("EndInventoryList\n")[:-1]:
        head, *rest = block.split("\n")
        out[head.split()[1]] = [l[5:] if l.startswith("Item ") else None for l in rest[1:] if l]
    return out
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
            + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, 0, 0, 0, 72, 5))

# The first to log in is the administrator, who may teleport.
c = Client("Explorer", port); c.login(); c.playerpos(); c.pump(0.3)
c.take(0x34); say(c, f"/teleport {front[0]},{front[1]},{front[2]}")
mv = c.wait_for(0x34); c.pos_bs = struct.unpack_from(">fff", mv, 2); c.playerpos(); c.pump(0.3)
print("teleported to", [round(v / 10, 1) for v in c.pos_bs])
# The map block with the chest: a chest, standing on the dungeon's floor.
blk = tuple(v // 16 for v in chest)
below = (chest[0], chest[1] - 1, chest[2])
end = time.time() + 30; got = None
while time.time() < end and got is None:
    c.pump(0.1)
    for m in c.take(0x20):
        if struct.unpack_from(">hhh", m, 2) == blk: got = m
assert got, "never got the chest's block"
print("chest's node", block_node(got, chest), "floor", block_node(got, below) if below[1] // 16 == blk[1] else "(next block)")
assert block_node(got, chest) == CHEST
if below[1] // 16 == blk[1]:
    assert block_node(got, below) in (COBBLE, MOSSY, DESERT_STONE)
# Open it: the loot is there.
c.cmds.clear(); interact(c, 3, 0, chest, front)
det = c.wait_for(0x43); name = det[4:4 + u16(det, 2)].decode()
loot = dlists(det)["main"]
items = [(i, x) for i, x in enumerate(loot) if x]
print("opened", name, "holding", [x for _, x in items])
assert items, "the chest was empty"
# Take the first stack.
slot, item = items[0]
c.cmds.clear(); c.inventory_action(f"Move {item.split()[1] if len(item.split()) > 1 else 1} detached:{name} main {slot} current_player main 7")
inv = lists(c.wait_for(0x27))
print("took", item, "->", inv["main"][7])
assert inv["main"][7] and inv["main"][7].split()[0] == item.split()[0]
# Back again later: the same loot, less what was taken.
c.raw(0, b"\x00\x03"); time.sleep(0.5)
d = Client("Explorer", port); d.login_existing(); d.playerpos(); d.pump(0.5)
d.cmds.clear(); interact(d, 3, 0, chest, front)
again = dlists(d.wait_for(0x43))["main"]
print("again:", [x for x in again if x])
assert again[slot] is None and [x for x in again if x] == [x for _, x in items[1:]]
print("DUNGEON OK")
d.raw(0, b"\x00\x03"); time.sleep(0.3)
