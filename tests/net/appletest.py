import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "picker.inv"), "w").write("kit 1\n")
open(os.path.join(players, "picker.player"), "w").write("pos 21.00 3.50 0.00\nlook 0.0 0.0\nhp 10\nskin default\n")
c = Client("Picker", port); c.login()
c.playerpos(); c.pump(0.5)
apple = (23, 5, 0)
def interact(action, item, under, above):
    pointed = struct.pack(">BB3h3h", 0, 1, *under, *above) if under else struct.pack(">BB", 0, 0)
    p = [int(v * 100) for v in c.pos_bs]
    c.send(0, struct.pack(">HBH", 0x39, action, item) + struct.pack(">I", len(pointed)) + pointed + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, 0, 0, 0, 72, 5))
c.cmds.clear()
interact(2, 0, apple, (22, 5, 0))   # pick it by hand
mark = c.wait_for(0x21, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == apple)
print("picked: node there now", u16(mark, 8)); assert u16(mark, 8) == 25
inv = c.wait_for(0x27, 5, cond=lambda m: b"esp:apple" in m)
print("apple in inventory")
c.cmds.clear()
interact(4, 0, None, None)   # eat (left-click with the apple)
hp = c.wait_for(0x33, 5); print("HP now", u16(hp, 2)); assert u16(hp, 2) == 12
inv = c.wait_for(0x27, 5); assert b"esp:apple" not in inv
# The mark grows a new apple (LUANTI_GROW_SECS shortens the wait).
t0 = time.time(); back = None
while time.time() - t0 < 10 and not back:
    c.raw(0, b"\x00\x02"); c.pump(0.5)
    back = [m for m in c.take(0x21) if struct.unpack_from(">hhh", m, 2) == apple and u16(m, 8) == 24]
print("apple grew back:", bool(back)); assert back
c.raw(0, b"\x00\x03"); time.sleep(0.5)
print("APPLES OK")
