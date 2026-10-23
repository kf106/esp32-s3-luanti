"""A player who leaves underground comes back there: python3 deeptest.py PORT netclient.py"""
import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
world = os.environ["TEST_WORLD"]
os.makedirs(world + "/players", exist_ok=True)
open(world + "/players/mole.inv", "w").write("kit 1\n0 esp:pick_steel 1\n1 esp:shovel_steel 1\n")
def dig(cl, item, p):
    pointed = struct.pack(">BB3h3h", 0, 1, *p, p[0], p[1] + 1, p[2])
    q = [int(v * 100) for v in cl.pos_bs]
    cl.send(0, struct.pack(">HBH", 0x39, 2, item) + struct.pack(">I", len(pointed)) + pointed
            + struct.pack(">3i3iiiIBB", *q, 0, 0, 0, 0, 0, 0, 72, 5))
c = Client("Mole", port); c.login(); c.pump(2)
px, py, pz = (v / 10 for v in c.pos_bs)
gx, gz, gy = round(px), round(pz), round(py - 0.6)
# Dig a shaft 7 deep beside spawn, and stand at its bottom.
x, z = gx + 2, gz
for y in range(gy, gy - 7, -1):
    c.pos_bs = [x * 10, (y + 0.5) * 10, z * 10]
    for item in (1, 0):
        dig(c, item, (x, y, z))
    c.pump(0.3)
feet = gy - 6.5
c.pos_bs = [x * 10, feet * 10, z * 10]; c.playerpos(); c.pump(0.5)
c.raw(0, b"\x00\x03"); time.sleep(1)
again = Client("Mole", port); again.login_existing()
print("left with feet at", feet, "- back at", [round(v / 10, 2) for v in again.pos_bs])
assert abs(again.pos_bs[1] / 10 - feet) < 0.01, "sent back to the surface"
again.raw(0, b"\x00\x03"); time.sleep(0.3); print("DEEP RETURN OK")
