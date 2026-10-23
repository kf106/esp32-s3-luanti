import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "lumberjack.inv"), "w").write("kit 1\n0 esp:axe_steel 1\n1 esp:birch_sapling 3\n")
c = Client("Lumberjack", port); c.login()
c.pos_bs = (110.0, 50.0, -20.0); c.playerpos(); c.pump(0.5)
def interact(action, item, under, above):
    pointed = struct.pack(">BB3h3h", 0, 1, *under, *above)
    p = [int(v * 100) for v in c.pos_bs]
    c.send(0, struct.pack(">HBH", 0x39, action, item) + struct.pack(">I", len(pointed)) + pointed + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, 0, 0, 0, 72, 5))
c.cmds.clear()
for y in range(5, 10):
    interact(2, 0, (11, y, -4), (11, y, -3))
    rm = c.wait_for(0x22, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == (11, y, -4))
print("trunk chopped")
removed = []; items = 0; t0 = time.time()
while time.time() - t0 < 16:
    c.raw(0, b"\x00\x02"); c.pump(1.0)
    removed += [struct.unpack_from(">hhh", m, 2) for m in c.take(0x22)]
    for m in c.take(0x31):
        nrem = u16(m, 2); o = 4 + 2 * nrem; nadd = u16(m, o); items += nadd
first = None
print(f"{len(removed)} leaves decayed within 16 s; {items} saplings dropped")
print("sample:", sorted(removed)[:6])
assert len(removed) >= 20
assert all(abs(x - 11) <= 3 and abs(z + 4) <= 3 for x, y, z in removed)
# Plant a sapling on grass nearby; placed on air it is refused.
c.cmds.clear()
interact(3, 1, (13, 4, -1), (13, 5, -1))
add = c.wait_for(0x21, 5); print("planted node", u16(add, 8), "at", struct.unpack_from(">hhh", add, 2))
assert u16(add, 8) == 20
if os.environ.get("GROW"):
    grown = []; t0 = time.time()
    while time.time() - t0 < 12 and not any(u16(m, 8) == 11 for m in grown):
        c.raw(0, b"\x00\x02"); c.pump(0.5)
        grown += c.take(0x21)
    end = time.time() + 1
    while time.time() < end: c.pump(0.2); grown += [m for m in c.take(0x21)]
    print(f"sapling grew: {sum(u16(m, 8) == 11 for m in grown)} trunk and {sum(u16(m, 8) == 12 for m in grown)} leaf nodes; trunk at", sorted(struct.unpack_from(">hhh", m, 2) for m in grown if u16(m, 8) == 11)[:3])
    assert any(struct.unpack_from(">hhh", m, 2) == (13, 5, -1) and u16(m, 8) == 11 for m in grown)
c.raw(0, b"\x00\x03"); time.sleep(0.5)
print("LEAF DECAY OK")
