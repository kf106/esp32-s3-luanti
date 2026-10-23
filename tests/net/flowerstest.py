"""Waterlily, mushrooms and wool over the network: python3 flowerstest.py PORT netclient.py X,Y,Z (open water)."""
import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
water = tuple(int(v) for v in sys.argv[3].split(","))
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "florist.inv"), "w").write("kit 1\n0 esp:waterlily 2\n1 esp:mushroom_red 1\n2 esp:rose 1\n3 esp:wool_white 1\n")
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
c = Client("Florist", port); c.login(); c.playerpos(); c.pump(0.3)
lists(c.wait_for(0x27))
# A rose in the craft grid: four red dye; one with white wool: red wool.
c.cmds.clear(); c.inventory_action("Move 1 current_player main 2 current_player craft 0"); inv = lists(c.wait_for(0x27))
print("a rose:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:dye_red 4"]
c.cmds.clear(); c.inventory_action("Craft 1 current_player"); inv = lists(c.wait_for(0x27))
c.cmds.clear(); c.inventory_action("Move 1 current_player craftresult 0 current_player craft 4"); inv = lists(c.wait_for(0x27))
c.cmds.clear(); c.inventory_action("MoveSomewhere 3 current_player craftresult 0 current_player main"); inv = lists(c.wait_for(0x27))
c.cmds.clear(); c.inventory_action("Move 1 current_player main 3 current_player craft 5"); inv = lists(c.wait_for(0x27))
print("red dye and white wool:", inv["craftpreview"]); assert inv["craftpreview"] == ["esp:wool_red 1"]
# Eat a red mushroom: 5 HP lost.
c.cmds.clear(); interact(c, 4, 1, (0, 0, 0), (0, 1, 0))
hp = c.wait_for(0x33); print("HP after a red mushroom:", u16(hp, 2)); assert u16(hp, 2) == 15
# A waterlily on open water; not on dry land.
c.pos_bs = [water[0] * 10, (water[1] + 2) * 10, water[2] * 10]; c.playerpos()
want = tuple(v // 16 for v in water)
end = time.time() + 20
while time.time() < end and not any(struct.unpack_from(">hhh", m, 2) == want for m in c.take(0x20)):
    c.playerpos(); c.pump(0.2)
above = (water[0], water[1] + 1, water[2])
c.cmds.clear(); interact(c, 3, 0, water, above)
m = c.wait_for(0x21, cond=lambda m: struct.unpack_from(">hhh", m, 2) == above)
print("waterlily:", u16(m, 8), "turned", m[11]); assert u16(m, 8) == 342
c.raw(0, b"\x00\x03"); time.sleep(0.3); print("FLOWERS OK")
