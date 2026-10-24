"""The weather's clouds and shadows, and env_sounds' running water and
lava, over the network: python3 weathertest.py PORT netclient.py"""
import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
world = os.environ["TEST_WORLD"]
os.makedirs(world + "/players", exist_ok=True)
open(world + "/players/listener.inv", "w").write("kit 1\n0 esp:bucket_water 1\n1 esp:bucket_lava 1\n")

def interact(cl, action, item, under, above):
    pointed = struct.pack(">BB3h3h", 0, 1, *under, *above)
    p = [int(v * 100) for v in cl.pos_bs]
    cl.send(0, struct.pack(">HBH", 0x39, action, item) + struct.pack(">I", len(pointed)) + pointed
            + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, 3000, 0, 0, 72, 5))
def clouds(m):
    density, bright, ambient, height, thickness, sx, sz, shadow = struct.unpack_from(">fIIffffI", m, 2)
    return density, bright, ambient, height, thickness, (sx, sz), shadow
def sound(m):
    sid = struct.unpack_from(">i", m, 2)[0]; n = u16(m, 6); name = m[8:8 + n].decode()
    gain, kind = struct.unpack_from(">fB", m, 8 + n); pos = struct.unpack_from(">fff", m, 13 + n)
    return name, gain, tuple(v / 10 for v in pos), sid
def heard(cl, name, secs):
    end = time.time() + secs
    while time.time() < end:
        cl.pump(0.05)
        for m in cl.take(0x3f):
            s = sound(m)
            if s[0] == name: return s
    return None

c = Client("Listener", port); c.login(); c.playerpos()
# On joining: the clouds and the lighting.
d, bright, ambient, height, thick, speed, shadow = clouds(c.wait_for(0x54))
print(f"clouds: density {d:.3f}, thickness {thick}, speed ({speed[0]:.2f}, {speed[1]:.2f}), height {height}")
assert 0 <= d <= 1 and 2 <= thick <= 32 and thick == int(thick) and abs(speed[0]) <= 4 and abs(speed[1]) <= 4
assert (bright, ambient, height, shadow) == (0xE5F0F0FF, 0xFF000000, 120.0, 0xFFCCCCCC)
light = c.wait_for(0x63)
intensity, saturation = struct.unpack_from(">ff", light, 2)
volumetric, = struct.unpack_from(">f", light, 2 + 8 + 24)
bloom, = struct.unpack_from(">f", light, 2 + 8 + 24 + 4 + 4)
print(f"shadows {intensity:.3f}, volumetric {volumetric}, bloom {bloom:.2f}, {len(light) - 2} bytes")
assert abs(intensity - 0.7 * (1 - d)) < 1e-5 and saturation == 1 and abs(volumetric - 0.2) < 1e-6 and abs(bloom - 0.05) < 1e-6
assert len(light) - 2 == 4 * 15 + 4
# Every 8 s, again.
c.cmds.clear(); c.wait_for(0x54, 10); print("clouds again")
# No running water yet.
assert heard(c, "env_sounds_water", 4) is None
# A bucket of water poured beside: it flows, and runs.
px, py, pz = (v / 10 for v in c.pos_bs)
gx, gz, gy = round(px), round(pz), round(py - 0.6)
spot = (gx + 3, gy + 1, gz)
interact(c, 3, 0, (spot[0], gy, spot[2]), spot)
s = heard(c, "env_sounds_water", 8); print("heard", s)
assert s and 0.04 < s[1] <= 0.4 and s[3] == -1 and abs(s[2][1] - gy - 1) < 1.5
# Lava further off, the other way: bubbling too.
spot = (gx - 4, gy + 1, gz)
interact(c, 3, 1, (spot[0], gy, spot[2]), spot)
s = heard(c, "env_sounds_lava", 8); print("heard", s)
assert s and 0.008 <= s[1] <= 0.6 and s[2][0] < gx
c.raw(0, b"\x00\x03"); print("WEATHER OK")
