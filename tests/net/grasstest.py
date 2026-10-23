import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "digger.inv"), "w").write("kit 1\n0 esp:shovel_steel 1\n")
def interact(cl, action, item, under, above):
    pointed = struct.pack(">BB3h3h", 0, 1, *under, *above)
    p = [int(v * 100) for v in cl.pos_bs]
    cl.send(0, struct.pack(">HBH", 0x39, action, item) + struct.pack(">I", len(pointed)) + pointed
            + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, 3000, 0, 0, 72, 5))
c = Client("Digger", port); c.login(); c.playerpos(); c.pump(1.0)
# The flat ground south of spawn is at y 2 (x 6 to 14, z 8 to 14). Dig it
# all; where tall grass stood on it, the grass must go too.
fell = []
for z in range(9, 15):
    for x in range(6, 15):
        if (x, z) == (8, 8): continue
        c.cmds.clear(); interact(c, 2, 0, (x, 2, z), (x, 3, z)); c.pump(0.15)
        removed = [struct.unpack_from(">hhh", m, 2) for m in c.take(0x22)]
        if (x, 3, z) in removed: fell.append((x, z))
print("grass fell with its ground at", fell); assert fell, "no grass in the test area?"
c.raw(0, b"\x00\x03"); time.sleep(0.3); print("GRASS OK")
