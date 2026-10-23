"""Lava over the network: python3 lavatest.py PORT netclient.py."""
import sys, time, struct, os, zstandard
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
world = os.environ["TEST_WORLD"]
os.makedirs(world + "/players", exist_ok=True)
open(world + "/players/burner.inv", "w").write("kit 1\n0 esp:bucket_lava 1\n1 esp:bucket_water 1\n2 esp:bucket_empty 1\n")

def interact(cl, action, item, under, above):
    pointed = struct.pack(">BB3h3h", 0, 1, *under, *above)
    p = [int(v * 100) for v in cl.pos_bs]
    cl.send(0, struct.pack(">HBH", 0x39, action, item) + struct.pack(">I", len(pointed)) + pointed
            + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, 3000, 0, 0, 72, 5))
state = {}
def gather(cl, secs):
    """Nodes as last seen, from node commands and whole blocks."""
    addnodes = []
    end = time.time() + secs
    while time.time() < end:
        for m in cl.cmds:
            cmd = u16(m)
            if cmd == 0x21:
                p = struct.unpack_from(">hhh", m, 2); state[p] = (u16(m, 8), m[11]); addnodes.append((p, u16(m, 8)))
            elif cmd == 0x22:
                state[struct.unpack_from(">hhh", m, 2)] = (126, 0)
            elif cmd == 0x20:
                bx, by, bz = struct.unpack_from(">hhh", m, 2)
                raw = zstandard.ZstdDecompressor().decompressobj().decompress(m[8:])
                for i in range(4096):
                    z, y, x = i // 256, (i // 16) % 16, i % 16
                    state[(bx * 16 + x, by * 16 + y, bz * 16 + z)] = (struct.unpack_from(">H", raw, 5 + 2 * i)[0], raw[5 + 12288 + i])
        cl.cmds = [m for m in cl.cmds if u16(m) not in (0x20, 0x21, 0x22)]
        cl.pump(0.05)
    return addnodes

c = Client("Burner", port); c.login()
px, py, pz = (v / 10 for v in c.pos_bs)
gx, gz, gy = round(px), round(pz), round(py - 0.6)
gather(c, 3)
spot = (gx + 3, gy + 1, gz + 3)
# Pour lava: it arrives as a whole block, never as a single node.
c.cmds.clear(); interact(c, 3, 0, (spot[0], gy, spot[2]), spot)
adds = gather(c, 2)
print("lava poured:", state.get(spot)); assert state.get(spot, (0,))[0] == 135
assert not any(i in (135, 136) for _, i in adds), f"lava sent node by node: {adds}"
# It creeps out. The burner stands clear, above it: dying would leave
# their buckets in bones.
c.pos_bs = [px * 10, (py + 3) * 10, pz * 10]; c.playerpos()
gather(c, 8)
flowing = [p for p, v in state.items() if v[0] == 136]
print("flowing lava after 10 s:", len(flowing), "nodes, e.g.", flowing[:3]); assert flowing
# Someone with nothing on them steps into it (so no bones are left in
# the lava): 8 HP a second.
d = Client("Diver", port); d.login(); d.playerpos(); d.pump(0.3)
d.inventory_action("Drop 0 current_player main 0"); d.pump(0.5)
d.cmds.clear(); d.pos_bs = [spot[0] * 10, (spot[1] - 0.5) * 10, spot[2] * 10]; d.playerpos()
hp = []
end = time.time() + 2.5
while time.time() < end:
    for m in d.take(0x33): hp.append(u16(m, 2))
    d.pump(0.05)
    c.pump(0.01)
print("HP in lava:", hp); assert hp and hp[0] == 12
# Burnt to death: respawn.
d.wait_for(0x37, 5); d.send(0, struct.pack(">H", 0x38)); d.wait_for(0x34, 5)
d.raw(0, b"\x00\x03")
c.playerpos(); assert not c.take(0x37), 'the burner died'
# Water beside the source: it cools to obsidian, with a hiss (2 s, chance
# 1 in 2 each time).
wspot = (spot[0] - 1, gy + 1, spot[2])
c.cmds.clear(); interact(c, 3, 1, (wspot[0], gy, wspot[2]), wspot)
sounds = []
end = time.time() + 20
while time.time() < end and state.get(spot, (0,))[0] != 137:
    gather(c, 0.5)
    for m in c.take(0x3f): sounds.append(m[8:8 + u16(m, 6)].decode())
print("lava source beside water became:", state.get(spot), "sounds:", sounds, "water spot:", state.get(wspot))
assert state.get(spot, (0,))[0] == 137 and "default_cool_lava" in sounds
c.raw(0, b"\x00\x03"); time.sleep(0.3); print("LAVA OK")
