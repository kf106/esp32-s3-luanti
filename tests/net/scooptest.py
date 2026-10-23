import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
world = os.environ["TEST_WORLD"]
os.makedirs(world + "/players", exist_ok=True)
open(world + "/players/scooper.inv", "w").write("kit 1\n0 esp:bucket_lava 1\n1 esp:bucket_empty 1\n")
def interact(cl, action, item, under, above):
    pointed = struct.pack(">BB3h3h", 0, 1, *under, *above)
    p = [int(v * 100) for v in cl.pos_bs]
    cl.send(0, struct.pack(">HBH", 0x39, action, item) + struct.pack(">I", len(pointed)) + pointed
            + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, 3000, 0, 0, 72, 5))
c = Client("Scooper", port); c.login(); c.pump(2)
px, py, pz = (v / 10 for v in c.pos_bs)
gx, gz, gy = round(px), round(pz), round(py - 0.6)
spot = (gx + 3, gy + 1, gz + 3)
c.cmds.clear(); interact(c, 3, 0, (spot[0], gy, spot[2]), spot); c.pump(1.5)
c.cmds.clear(); interact(c, 4, 1, spot, (spot[0], spot[1] + 1, spot[2]))
inv = c.wait_for(0x27, 5, cond=lambda m: b"bucket_lava" in m)
print("after scooping:", [l for l in inv.decode(errors="replace").split("\n") if "bucket" in l])
c.raw(0, b"\x00\x03"); time.sleep(0.3); print("SCOOP OK")
