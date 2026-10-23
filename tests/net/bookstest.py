"""Writing in books over the network, on a fresh world:
python3 bookstest.py PORT netclient.py"""
import sys, time, struct, os, zlib
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
exec(src[src.index("def formspec(c)"):src.index('c = Client("Skinner"')])
port = int(sys.argv[1])
players = os.path.join(os.environ["TEST_WORLD"], "players")
os.makedirs(players, exist_ok=True)
open(os.path.join(players, "writer.inv"), "w").write("kit 1\n0 esp:book 2\n2 esp:bookshelf 1\n")

def lists(c):
    text = c[6:6 + struct.unpack_from(">I", c, 2)[0]].decode()
    out = {}
    for block in text.split("EndInventoryList\n")[:-1]:
        head, *rest = block.split("\n")
        out[head.split()[1]] = [l[5:] if l.startswith("Item ") else None for l in rest[1:] if l]
    return out
def dlists(c):
    n = u16(c, 2); text = c[2 + 2 + n + 1 + 2:].decode()
    out = {}
    for block in text.split("EndInventoryList\n")[:-1]:
        head, *rest = block.split("\n")
        out[head.split()[1]] = [l[5:] if l.startswith("Item ") else None for l in rest[1:] if l]
    return out
def show(c):
    """TOCLIENT_SHOW_FORMSPEC: (formspec, formname)."""
    n = struct.unpack_from(">I", c, 2)[0]; fs = c[6:6 + n].decode()
    m = u16(c, 6 + n); return fs, c[8 + n:8 + n + m].decode()
def use(cl, slot):
    pointed = b"\x00\x00"  # pointing at nothing
    p = [int(v * 100) for v in cl.pos_bs]
    cl.send(0, struct.pack(">HBH", 0x39, 4, slot) + struct.pack(">I", len(pointed)) + pointed
            + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, 3000, -9000, 0, 72, 5))
def interact(cl, action, item, under, above):
    pointed = struct.pack(">BB3h3h", 0, 1, *under, *above)
    p = [int(v * 100) for v in cl.pos_bs]
    cl.send(0, struct.pack(">HBH", 0x39, action, item) + struct.pack(">I", len(pointed)) + pointed
            + struct.pack(">3i3iiiIBB", *p, 0, 0, 0, 3000, -9000, 0, 72, 5))
def send_fields(cl, form, values):
    body = struct.pack(">H", len(form)) + form.encode() + struct.pack(">H", len(values))
    for k, v in values.items():
        kb, vb = k.encode(), v.encode()
        body += struct.pack(">H", len(kb)) + kb + struct.pack(">I", len(vb)) + vb
    cl.send(0, struct.pack(">H", 0x3c) + body)
def written(item):
    """The values in a written book's item string."""
    name, count, wear, meta = item.split(" ", 3)
    raw = json.loads(meta) if meta.startswith('"') else meta
    return dict(p.split("\x02", 1) for p in raw[1:].split("\x03") if p)
import json

w = Client("Writer", port); w.login(); w.playerpos(); w.pump(0.3)
lists(w.wait_for(0x27))
# Using a new book: the window to write in.
w.cmds.clear(); use(w, 0)
fs, name = show(w.wait_for(0x44)); print("window", name, fs[:60])
assert name == "default:book" and "field[0.5,1;7.5,0;title;Title:;]" in fs and "tabheader" not in fs
text = "Day one.\nIt rained; we built a [hut], and\tsaw $tars."
send_fields(w, "default:book", {"title": "My  Journal", "text": text, "save": "Save", "quit": "true"})
inv = lists(w.wait_for(0x27))["main"]
print("after saving:", inv[:3]); assert inv[0] == "esp:book 1" and inv[1].startswith("esp:book_written 1 0 ")
data = written(inv[1]); print(data)
assert data["title"] == "My  Journal" and data["text"] == text and data["owner"] == "Writer"
assert data["description"] == '"My  Journal" by Writer' and data["page_max"] == "1"
# The author gets tabs to write or read; the text comes back escaped.
w.cmds.clear(); use(w, 1)
fs, _ = show(w.wait_for(0x44))
assert "tabheader[0,0;book_header;Write,Read;1;false;false]" in fs and "\\; we built a \\[hut\\]\\, and\tsaw \\$tars." in fs
send_fields(w, "default:book", {"book_header": "2"})
fs, _ = show(w.wait_for(0x44)); print("read tab:", "Page 1 of 1" in fs); assert "Page 1 of 1" in fs and "by Writer" in fs
send_fields(w, "default:book", {"quit": "true"}); w.pump(0.3)
# On a bookshelf: one written book.
px, py, pz = (v / 10 for v in w.pos_bs)
gx, gz, gy = round(px), round(pz), round(py - 0.6)
spot = (gx + 2, gy + 1, gz); up = (spot[0], spot[1] + 1, spot[2])
w.cmds.clear(); interact(w, 3, 2, (spot[0], gy, spot[2]), spot)
w.wait_for(0x21, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == spot)
w.cmds.clear(); interact(w, 3, 0, spot, up)
det = w.wait_for(0x43); loc = "detached:" + det[4:4 + u16(det, 2)].decode()
w.cmds.clear(); w.inventory_action(f"Move 1 current_player main 1 {loc} main 0")
shelf = dlists(w.wait_for(0x43))["main"]; print("on the shelf:", shelf[0][:40]); assert shelf[0] == inv[1]
w.send(0, fields("esp:chest", "quit")); w.pump(0.3)
# Someone else takes it down and reads it: no tabs, and they can't change it.
r = Client("Reader", port); r.login(); r.pos_bs = w.pos_bs; r.playerpos(); r.pump(0.5)
r.cmds.clear(); interact(r, 3, 0, spot, up); r.wait_for(0x43)
r.cmds.clear(); r.inventory_action(f"Move 1 {loc} main 0 current_player main 7")
got = lists(r.wait_for(0x27, cond=lambda m: b"book_written" in m))["main"][7]; assert got == inv[1]
r.send(0, fields("esp:chest", "quit")); r.pump(0.3)
r.cmds.clear(); use(r, 7)
fs, _ = show(r.wait_for(0x44)); print("reader sees:", fs[:70])
assert "label[0.5,0.5;by Writer]" in fs and "tabheader" not in fs and "Page 1 of 1" in fs
send_fields(r, "default:book", {"title": "Mine now", "text": "x", "save": "Save", "quit": "true"}); r.pump(0.5)
r.raw(0, b"\x00\x03"); time.sleep(0.5)
# Back again later: the book, as written, in the inventory.
r = Client("Reader", port); r.login_existing(); r.pump(0.3)
again = lists(r.wait_for(0x27))["main"][7]; print("after reconnecting:", written(again)["title"])
assert again == inv[1]
for cl in (w, r): cl.raw(0, b"\x00\x03")
time.sleep(0.3); print("BOOKS OK")
