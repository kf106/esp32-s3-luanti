"""Walls over the network: python3 wallstest.py PORT netclient.py"""
import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
WALL_COBBLE = 370
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "mason.inv"), "w").write("kit 1\n0 esp:cobble 6\n1 esp:desert_cobble 6\n2 esp:pick_stone 1\n")

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

c = Client("Mason", port); c.login(); c.playerpos(); c.pump(0.3)
lists(c.wait_for(0x27))
# Two rows of three cobblestone: six walls.
for i in range(6):
    c.cmds.clear(); c.inventory_action(f"Move 1 current_player main 0 current_player craft {i + 3}"); inv = lists(c.wait_for(0x27))
print("six cobblestone:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:wall_cobble 6"]
c.cmds.clear(); c.inventory_action("Craft 1 current_player"); lists(c.wait_for(0x27))
c.cmds.clear(); c.inventory_action("MoveSomewhere 6 current_player craftresult 0 current_player main"); inv = lists(c.wait_for(0x27))
slot = next(i for i, x in enumerate(inv["main"]) if x and x.startswith("esp:wall_cobble"))
print("walls in slot", slot, ":", inv["main"][slot])
px, py, pz = (v / 10 for v in c.pos_bs)
gx, gz, gy = round(px), round(pz), round(py - 0.6)
spot = (gx + 2, gy + 1, gz)
c.cmds.clear(); interact(c, 3, slot, (spot[0], gy, spot[2]), spot)
m = c.wait_for(0x21, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == spot)
print("placed:", u16(m, 8)); assert u16(m, 8) == WALL_COBBLE
c.cmds.clear(); interact(c, 2, 2, spot, (spot[0], spot[1] + 1, spot[2]))
c.wait_for(0x22, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == spot)
inv = lists(c.wait_for(0x27, 5, cond=lambda m: b"esp:wall_cobble 6" in m))["main"]
print("dug it back:", [x for x in inv if x]); assert "esp:wall_cobble 6" in inv
c.raw(0, b"\x00\x03"); time.sleep(0.3); print("WALLS OK")
