"""The crafting guide (the inventory window's Recipes tab) over the network:
python3 craftguidetest.py PORT netclient.py"""
import sys, struct
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
exec(src[src.index("def formspec(c)"):src.index('c = Client("Skinner"')])
port = int(sys.argv[1])

def send(cl, **values):
    """TOSERVER_INVENTORY_FIELDS for the inventory window ("")."""
    body = struct.pack(">HH", 0, len(values))
    for n, v in values.items():
        n = n.replace("__", ":").replace("_dot_", ".")
        body += struct.pack(">H", len(n)) + n.encode() + struct.pack(">I", len(v.encode())) + v.encode()
    cl.cmds.clear(); cl.send(0, struct.pack(">H", 0x3c) + body)
    return formspec(cl.wait_for(0x42))
def label_of(fs, x):
    return fs.split(f"label[{x},")[1].split(";", 1)[1].split("]")[0]

c = Client("Reader", port); c.login()
fs = formspec(c.wait_for(0x42))
assert "tabheader[0,0;sfinv_nav_tabs;Crafting,Recipes;1;true;false]" in fs and "model[" in fs, fs
# The Recipes tab: the first page of items, sorted by name.
fs = send(c, sfinv_nav_tabs="2")
assert "Crafting,Recipes;2;" in fs and "craftguide_search_icon.png" in fs and "model[" not in fs, fs
items = [p.split(";")[2] for p in fs.split("item_image_button[")[1:]]
print(len(items), "items on page 1:", items[:5], "...")
assert len(items) == 48 and items == sorted(items), items
print("page", label_of(fs, "8.05"))
# Next page, and back round from the first to the last.
fs = send(c, next="")
assert "\x1b(c@yellow)2\x1b(c@#ffffff)" in fs
fs = send(c, prev=""); fs = send(c, prev="")
last = label_of(fs, "8.05"); print("from page 1 back to", last)
assert last.split(")")[1].split("\x1b")[0] == last.split(" / ")[1]
# Searching, by Enter in the box.
fs = send(c, filter="Torch", key_enter_field="filter")
items = [p.split(";")[2] for p in fs.split("item_image_button[")[1:]]
print("torch:", items); assert "esp:torch" in items and all("torch" in i for i in items)
# The torch's recipe: coal above a stick (group:stick, a G).
fs = send(c, esp__torch="", filter="torch")
print(label_of(fs, "10.6")); assert label_of(fs, "10.6") == "Recipe 1 of 1"
assert ";esp:coal_lump;esp:coal_lump;]" in fs and ";esp:stick;esp:stick.stick;\nG]" in fs, fs
assert "tooltip[esp:stick.stick;Any stick]" in fs and "esp:torch 4;esp:torch;]" in fs
# Clicked again: what a torch goes into (none in minetest_game) ...
fs = send(c, esp__torch="", filter="torch")
print(label_of(fs, "10.6")); assert "No usages." in fs
# ... the stick, clicked in the recipe: its usages, shown as a stick.
fs = send(c, esp__stick_dot_stick="", filter="torch")
fs = send(c, esp__stick="", filter="torch")
n = int(label_of(fs, "10.6").split(" of ")[1]); print("stick usages:", n); assert n > 10
assert "Usage 1 of" in fs and "recipe_next" in fs
fs = send(c, recipe_next="", filter="torch"); assert f"Usage 2 of {n}" in fs
# Glass: the furnace, from any sand, in 3 s.
fs = send(c, clear="")
assert "field[0.5,8.75;3.4,0.8;filter;;]" in fs
fs = send(c, esp__glass="")
fur = [l for l in fs.split("]") if "craftguide_furnace" in l or "Cooking time" in l]
print(label_of(fs, "10.6"), fur); assert "tooltip[esp:sand.sand;Any sand]" in fs and "Recipe 1 of 2" in fs and fur
# Back to the Crafting tab.
fs = send(c, sfinv_nav_tabs="1")
assert "model[" in fs and "Crafting,Recipes;1;" in fs
send_close = struct.pack(">HHH", 0x3c, 0, 1) + struct.pack(">H", 4) + b"quit" + struct.pack(">I", 4) + b"true"
c.cmds.clear(); c.send(0, send_close); c.pump(0.5); assert not c.take(0x42)
c.raw(0, b"\x00\x03"); print("CRAFTGUIDE OK")
