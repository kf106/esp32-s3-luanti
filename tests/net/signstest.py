"""Signs and node metadata over the network, on a fresh world:
python3 signstest.py PORT netclient.py"""
import sys, time, struct, os, zlib
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
exec(src[src.index("import zstandard"):src.index('g = Client("Digger"')])
port = int(sys.argv[1])
SIGN_WOOD = 383
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "scribe.inv"), "w").write("kit 1\n0 esp:sign_wall_wood 2\n1 esp:axe_steel 1\n2 esp:shovel_steel 1\n3 esp:dirt 5\n")

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
def chat(c):
    o = 4; n = u16(c, o); o += 2 + 2 * n; n = u16(c, o); o += 2
    return c[o:o + 2 * n].decode("utf-16-be")

def meta_entries(b, absolute):
    """NodeMetadataList::deSerialize: [(pos, {name: value})]."""
    o = 0; version = b[o]; o += 1
    if version == 0: return []
    assert version == 2, version
    count = u16(b, o); o += 2
    out = []
    for _ in range(count):
        if absolute:
            pos = struct.unpack_from(">hhh", b, o); o += 6
        else:
            p16 = u16(b, o); o += 2; pos = (p16 % 16, p16 // 16 % 16, p16 // 256)
        nvars = struct.unpack_from(">I", b, o)[0]; o += 4
        fields = {}
        for _ in range(nvars):
            n = u16(b, o); o += 2; name = b[o:o + n].decode(); o += n
            n = struct.unpack_from(">I", b, o)[0]; o += 4; value = b[o:o + n].decode(); o += n
            assert b[o] == 0; o += 1  # not private
            fields[name] = value
        assert b[o:o + 13] == b"EndInventory\n"; o += 13
        out.append((pos, fields))
    return out
def meta_changed(m):
    n = struct.unpack_from(">I", m, 2)[0]
    return meta_entries(zlib.decompress(m[6:6 + n]), True)
def block_meta(m):
    raw = zstandard.ZstdDecompressor().decompressobj().decompress(m[8:])
    return meta_entries(raw[5 + 4096 * 4:], False)
def send_fields(cl, pos, fields):
    body = struct.pack(">H3hH", 0x3b, *pos, 0) + struct.pack(">H", len(fields))
    for k, v in fields.items():
        kb, vb = k.encode(), v.encode()
        body += struct.pack(">H", len(kb)) + kb + struct.pack(">I", len(vb)) + vb
    cl.send(0, body)
def block_of(cl, pos, secs=20):
    blk = tuple(v // 16 for v in pos)
    end = time.time() + secs
    while time.time() < end:
        for m in cl.take(0x20):
            if struct.unpack_from(">hhh", m, 2) == blk: return m
        cl.pump(0.1)
    raise AssertionError(f"{cl.name} never got block {blk}")

c = Client("Scribe", port); c.login(); c.playerpos(); c.pump(0.3)
lists(c.wait_for(0x27))
px, py, pz = (v / 10 for v in c.pos_bs)
gx, gz, gy = round(px), round(pz), round(py - 0.6)
ground, spot = (gx + 2, gy, gz), (gx + 2, gy + 1, gz)
block_of(c, spot)
# Placed on the floor: a sign lying flat (wallmounted 1), with its text box.
c.cmds.clear(); interact(c, 3, 0, ground, spot)
m = c.wait_for(0x21, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == spot)
print("placed:", u16(m, 8), "param2", m[11]); assert (u16(m, 8), m[11]) == (SIGN_WOOD, 1)
meta = meta_changed(c.wait_for(0x59))
print("its metadata:", meta); assert meta == [(spot, {"formspec": "field[text;;${text}]"})]
# Written on: the text, and what shows when it is pointed at.
c.cmds.clear(); send_fields(c, spot, {"text": "Hello\nworld\x07", "quit": "true"})
meta = meta_changed(c.wait_for(0x59))
print("written:", meta); assert meta[0][1]["text"] == "Hello\nworld" and meta[0][1]["infotext"] == '"Hello\nworld"'
# Fields sent without closing the form change nothing; too long a text is refused.
c.cmds.clear(); send_fields(c, spot, {"text": "not yet"}); c.pump(0.5); assert not c.take(0x59)
send_fields(c, spot, {"text": "x" * 600, "quit": "true"})
end = time.time() + 3; said = []
while time.time() < end and not any("Text too long" in s for s in said):
    c.pump(0.1); said += [chat(m) for m in c.take(0x2F)]
print("too long:", said); assert any("Text too long" in s for s in said); assert not c.take(0x59)
# A newcomer gets it with the map block.
n = Client("Reader", port); n.login(); n.playerpos()
got = block_meta(block_of(n, spot))
print("newcomer's block:", got)
assert got == [(tuple(v % 16 for v in spot), {"formspec": "field[text;;${text}]", "text": "Hello\nworld", "infotext": '"Hello\nworld"'})]
# Dug: gone, and back in the inventory; the next newcomer's block has no metadata.
c.cmds.clear(); interact(c, 2, 1, spot, (spot[0], spot[1] + 1, spot[2]))
c.wait_for(0x22, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == spot)
inv = lists(c.wait_for(0x27, 5, cond=lambda m: b"esp:sign_wall_wood 2" in m))["main"]
print("dug:", [x for x in inv if x]); assert "esp:sign_wall_wood 2" in inv
n.raw(0, b"\x00\x03"); time.sleep(0.3)
n2 = Client("Reader2", port); n2.login(); n2.playerpos()
assert block_meta(block_of(n2, spot)) == []
# On a wall it hangs; dig the wall and it drops.
wall, block = (gx - 2, gy + 1, gz), (gx - 3, gy + 1, gz)
c.cmds.clear(); interact(c, 3, 3, (block[0], gy, gz), block)
c.wait_for(0x21, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == block)
c.cmds.clear(); interact(c, 3, 0, block, wall)
m = c.wait_for(0x21, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == wall)
print("on a wall: param2", m[11]); assert u16(m, 8) == SIGN_WOOD and m[11] in (2, 3, 4, 5)
c.cmds.clear(); interact(c, 2, 2, block, (block[0], block[1] + 1, block[2]))
c.wait_for(0x22, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == wall)
print("the wall's gone, and the sign with it")
c.raw(0, b"\x00\x03"); n2.raw(0, b"\x00\x03"); time.sleep(0.3); print("SIGNS OK")
