"""Bones over the network, on a fresh world: python3 bonestest.py PORT netclient.py"""
import sys, time, struct, os
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
BONES = 369
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "victim.inv"), "w").write("kit 1\n0 esp:cobble 50\n1 esp:pick_steel 1\n2 esp:apple 3\n")

def chat(c):
    o = 4; n = u16(c, o); o += 2 + 2 * n; n = u16(c, o); o += 2
    return c[o:o + 2 * n].decode("utf-16-be")
def say(cl, text):
    u = text.encode("utf-16-be"); cl.send(0, struct.pack(">HH", 0x32, len(u) // 2) + u)
def heard(cl, want, secs=4):
    lines = []; end = time.time() + secs
    while time.time() < end:
        cl.pump(0.05); lines += [chat(m) for m in cl.take(0x2F)]
        if any(want in l for l in lines): return next(l for l in lines if want in l)
    raise AssertionError(f"{cl.name} didn't hear {want!r}: {lines}")
def lists(c):
    text = c[6:6 + struct.unpack_from(">I", c, 2)[0]].decode()
    out = {}
    for block in text.split("EndInventoryList\n")[:-1]:
        head, *rest = block.split("\n")
        out[head.split()[1]] = [l[5:] if l.startswith("Item ") else None for l in rest[1:] if l]
    return out
def items(c): return [x for x in lists(c)["main"] if x]
def interact(cl, action, item, under, above):
    pointed = struct.pack(">BB3h3h", 0, 1, *under, *above)
    p = [int(v * 100) for v in cl.pos_bs]
    cl.send(0, struct.pack(">HBH", 0x39, action, item) + struct.pack(">I", len(pointed)) + pointed
            + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, 3000, -9000, 0, 72, 5))
def bones_at(cl, secs=5):
    end = time.time() + secs
    while time.time() < end:
        for m in cl.take(0x21):
            if u16(m, 8) == BONES: return struct.unpack_from(">hhh", m, 2)
        cl.pump(0.05)
    raise AssertionError(f"{cl.name}: no bones appeared")
def gone(cl, pos, secs=5):
    end = time.time() + secs
    while time.time() < end:
        if any(struct.unpack_from(">hhh", m, 2) == pos for m in cl.take(0x22)): return True
        cl.pump(0.05)
    raise AssertionError("the bones didn't go")

boss = Client("Boss", port); boss.login(); boss.playerpos(); boss.pump(0.3)
v = Client("Victim", port); v.login(); v.playerpos(); v.pump(0.3)
print("victim has:", items(v.wait_for(0x27)))
# Killed: the inventory goes into bones where they stood.
v.cmds.clear(); boss.cmds.clear(); say(boss, "/kill Victim"); heard(boss, "Victim has been killed.")
pos = bones_at(boss); print("bones at", pos, "; victim stood at", [round(c / 10, 1) for c in v.pos_bs])
inv = v.wait_for(0x27, cond=lambda m: True); print("victim now has:", items(inv)); assert items(inv) == []
# Someone else: can't open them, and punching them takes nothing.
boss.cmds.clear(); interact(boss, 3, 0, pos, pos); print("  boss right-clicks:", heard(boss, "fresh bones"))
boss.cmds.clear(); interact(boss, 0, 0, pos, pos); boss.pump(0.5); assert not any(struct.unpack_from(">hhh", m, 2) == pos for m in boss.take(0x22))
# Back to life; punching them gives everything back, and the bones block.
v.send(0, struct.pack(">H", 0x38)); mv = v.wait_for(0x34); v.pos_bs = struct.unpack_from(">fff", mv, 2); v.playerpos(); v.pump(0.3)
v.cmds.clear(); interact(v, 0, 0, pos, pos); gone(v, pos)
inv = items(v.wait_for(0x27, cond=lambda m: b"esp:bones" in m)); print("after punching the bones:", inv)
assert "esp:cobble 50" in inv and any(x.startswith("esp:pick_steel") for x in inv) and "esp:apple 3" in inv and "esp:bones 1" in inv
print("PUNCH OK")
# Again, with one item: opened with a right-click, the last item taken out makes them go.
say(boss, "/clearinv Victim"); heard(boss, "Cleared Victim's inventory."); time.sleep(1.3)
say(boss, "/give Victim torch 7"); heard(boss, "added to inventory of Victim"); time.sleep(1.3)
v.cmds.clear(); say(boss, "/kill Victim"); heard(boss, "has been killed"); pos = bones_at(v)
v.send(0, struct.pack(">H", 0x38)); mv = v.wait_for(0x34); v.pos_bs = struct.unpack_from(">fff", mv, 2); v.playerpos(); v.pump(0.3)
v.cmds.clear(); interact(v, 3, 0, pos, pos)
det = v.wait_for(0x43); name = det[4:4 + u16(det, 2)].decode(); print("opened", name)
v.cmds.clear(); v.inventory_action(f"Move 7 detached:{name} main 0 current_player main 0"); gone(v, pos)
inv = items(v.wait_for(0x27, cond=lambda m: b"esp:bones" in m)); print("after taking the torches:", inv); assert inv == ["esp:torch 7", "esp:bones 1"]
# Nothing can be put into bones: kill again, open, try putting something back.
v.cmds.clear(); say(boss, "/kill Victim"); heard(boss, "has been killed"); pos = bones_at(v)
v.send(0, struct.pack(">H", 0x38)); mv = v.wait_for(0x34); v.pos_bs = struct.unpack_from(">fff", mv, 2); v.playerpos(); v.pump(0.3)
time.sleep(1.3); say(boss, "/give Victim dirt 2"); heard(boss, "added to inventory of Victim")
v.cmds.clear(); interact(v, 3, 0, pos, pos); det = v.wait_for(0x43)
v.cmds.clear(); v.inventory_action(f"Move 2 current_player main 0 detached:{name.rsplit('_', 3)[0]}_{pos[0]}_{pos[1]}_{pos[2]} main 5")
inv = items(v.wait_for(0x27)); print("tried to put dirt in:", inv); assert "esp:dirt 2" in inv
print("BONES OK")
v.raw(0, b"\x00\x03"); boss.raw(0, b"\x00\x03"); time.sleep(0.3)
