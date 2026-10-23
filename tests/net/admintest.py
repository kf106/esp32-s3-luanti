"""Admin commands and privileges over the network, on a fresh world: python3 admintest.py PORT netclient.py"""
import sys, time, struct, os, hashlib
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
N = int(open(os.path.join(os.path.dirname(sys.argv[2]), "srp_client.py")).read().split('N = int("')[1].split('"')[0], 16)
H = lambda b: hashlib.sha256(b).digest()
mb = lambda n: n.to_bytes((n.bit_length() + 7) // 8, "big")
pad = lambda n: n.to_bytes(256, "big")
I = lambda b: int.from_bytes(b, "big")

def chat(c):
    o = 4; n = u16(c, o); o += 2 + 2 * n; n = u16(c, o); o += 2
    return c[o:o + 2 * n].decode("utf-16-be")
def privs(c):
    n = u16(c, 2); o = 4; out = []
    for _ in range(n):
        l = u16(c, o); out.append(c[o + 2:o + 2 + l].decode()); o += 2 + l
    return sorted(out)
def denied(c):
    l = u16(c, 3); return c[5:5 + l].decode()

def hello(cl):
    name = cl.name.encode()
    cl.send(0, struct.pack(">HBHHH", 0x02, 29, 0, 37, 53) + struct.pack(">H", len(name)) + name)
    end = time.time() + 10
    while time.time() < end:
        for c in cl.take(0x0A): return ("denied", denied(c))
        for c in cl.take(0x02): return ("hello", struct.unpack_from(">I", c, 7)[0])
        cl.pump(0.05)
    raise AssertionError("no hello")

def join(cl):
    cl.send(1, struct.pack(">H", 0x11) + struct.pack(">H", 0))
    cl.wait_for(0x3c)
    cl.send(1, struct.pack(">HH", 0x40, 0))
    ver = b"5.17.0"
    cl.send(1, struct.pack(">HBBBB", 0x43, 5, 17, 0, 0) + struct.pack(">H", len(ver)) + ver + struct.pack(">H", 11))
    mv = cl.wait_for(0x34)
    cl.pos_bs = struct.unpack_from(">fff", mv, 2)
    return privs(cl.wait_for(0x41))

def register(cl, pw):
    kind, v = hello(cl); assert kind == "hello" and v & 4, (kind, v)
    salt = os.urandom(16)
    x = I(H(salt + H(cl.name.lower().encode() + b":" + pw.encode())))
    vb = mb(pow(2, x, N))
    cl.send(0, struct.pack(">H", 0x50) + struct.pack(">H", 16) + salt + struct.pack(">H", len(vb)) + vb + b"\x00")
    cl.wait_for(0x03)
    return join(cl)

def login(cl, pw):
    """SRP login; returns the privileges, or ("denied", reason)."""
    kind, v = hello(cl)
    if kind == "denied": return (kind, v)
    assert v & 2, f"expected SRP, got {v}"
    name = cl.name.encode()
    a = I(os.urandom(32)); A = pow(2, a, N)
    cl.send(0, struct.pack(">H", 0x51) + struct.pack(">H", len(mb(A))) + mb(A) + b"\x01")
    sb = cl.wait_for(0x60, 20)
    o = 2; ls = u16(sb, o); salt = sb[o + 2:o + 2 + ls]; o += 2 + ls; lb = u16(sb, o); B = I(sb[o + 2:o + 2 + lb])
    u = I(H(pad(A) + pad(B))); k = I(H(pad(N) + pad(2)))
    x = I(H(salt + H(cl.name.lower().encode() + b":" + pw.encode())))
    S = pow((B - k * pow(2, x, N)) % N, a + u * x, N); K = H(mb(S))
    hx = bytes(p ^ q for p, q in zip(H(mb(N)), H(mb(2))))
    M = H(hx + H(name) + salt + mb(A) + mb(B) + K)
    cl.send(0, struct.pack(">H", 0x52) + struct.pack(">H", len(M)) + M)
    end = time.time() + 10
    while time.time() < end:
        for c in cl.take(0x0A): return ("denied", denied(c))
        if cl.take(0x03): return join(cl)
        cl.pump(0.05)
    raise AssertionError("no answer to the login")

def say(cl, text):
    u = text.encode("utf-16-be"); cl.send(0, struct.pack(">HH", 0x32, len(u) // 2) + u)
def heard(cl, want, secs=4):
    lines = []; end = time.time() + secs
    while time.time() < end:
        cl.pump(0.05); lines += [chat(m) for m in cl.take(0x2F)]
        if any(want in l for l in lines): return lines
    raise AssertionError(f"{cl.name} didn't hear {want!r}: {lines}")
last_said = {}
def ask(cl, text, want):
    # Luanti's chat limit (8 messages in 10 s) covers commands too.
    wait = last_said.get(cl.name, 0) + 1.3 - time.time()
    if wait > 0: cl.pump(wait)
    last_said[cl.name] = time.time()
    cl.cmds.clear(); say(cl, text); lines = heard(cl, want)
    line = next(l for l in lines if want in l); print(f"  {cl.name}: {text}  ->  {line}"); return line

ALL = sorted("interact shout basic_privs privs teleport bring settime server ban kick give password fly fast noclip home".split())

boss = Client("Boss", port); p = register(boss, "bosspw"); boss.playerpos()
print("Boss's privileges:", p); assert p == ALL
pleb = Client("Pleb", port); p = register(pleb, "pw"); pleb.playerpos()
print("Pleb's privileges:", p); assert p == ["interact", "shout"]
ask(pleb, "/admin", "The administrator of this server is Boss.")
ask(pleb, "/grant Pleb fly", "Your privileges are insufficient.")
ask(pleb, "/teleport 0,10,0", "missing privileges: teleport")
h = ask(pleb, "/help", "Available commands:"); assert "grant" in h and "teleport" not in h
ask(pleb, "/help kick", "/kick <name> [<reason>]: Kick a player (no permission)")
print("PLAYER LIMITS OK")

# Granting and revoking: the player's client is told.
pleb.cmds.clear()
ask(boss, "/grant Pleb fly, fast", "Privileges of Pleb: fast, fly, interact, shout")
heard(pleb, "Boss granted you privileges: fast, fly")
assert privs(pleb.wait_for(0x41)) == ["fast", "fly", "interact", "shout"]
ask(boss, "/grant Pleb wings", "Unknown privilege: wings")
ask(boss, "/grant Nobody fly", "Player Nobody does not exist.")
ask(boss, "/revoke Pleb shout", "Privileges of Pleb: fast, fly, interact")
pleb.cmds.clear(); say(pleb, "hello?"); heard(pleb, "You don't have permission to shout.")
ask(boss, "/revoke Boss fly", "Note: Cannot revoke from admin")
ask(boss, "/privs Pleb", "Privileges of Pleb: fast, fly, interact")
ask(boss, "/haspriv fly", "Players online with the \"fly\" privilege: Boss, Pleb")
# Without interact, a dug node comes back.
ask(boss, "/revoke Pleb interact", "Privileges of Pleb: fast, fly")
x, y, z = (round(v / 10) for v in pleb.pos_bs)
under = (x, y - 1, z)
pleb.cmds.clear()
pp = [int(v * 100) for v in pleb.pos_bs]
pointed = struct.pack(">BB3h3h", 0, 1, *under, x, y, z)
pleb.send(0, struct.pack(">HBH", 0x39, 2, 0) + struct.pack(">I", len(pointed)) + pointed + struct.pack(">3i3iiiIBB", *pp, 0, 0, 0, 3000, -9000, 0, 72, 5))
back = pleb.wait_for(0x21, 5, cond=lambda m: struct.unpack_from(">hhh", m, 2) == under)
print("  dig without interact: node", under, "sent back as", u16(back, 8))
ask(boss, "/grant Pleb interact, shout", "Privileges of Pleb: fast, fly, interact, shout")
print("PRIVILEGES OK")

# Items, time, teleporting.
ask(boss, "/giveme torch 5", "esp:torch 5 added to inventory.")
ask(boss, "/give Pleb default:cobble 3", "esp:cobble 3 added to inventory of Pleb.")
ask(boss, "/give Pleb unobtainium", "Cannot give an unknown item.")
pleb.cmds.clear()
ask(boss, "/time 6:00", "Time of day changed.")
pleb.wait_for(0x29)
ask(boss, "/time", "Current time is 6:0")
pleb.cmds.clear()
ask(boss, "/teleport Pleb 10,20,30", "Teleporting Pleb to (10.0, 20.0, 30.0).")
mv = pleb.wait_for(0x34); print("  Pleb moved to", struct.unpack_from(">fff", mv, 2)); assert struct.unpack_from(">fff", mv, 2) == (100.0, 200.0, 300.0)
# The others see Pleb there too (their client's next report matches, so
# it isn't relayed as a move).
boss.wait_for(0x32, 5, cond=lambda m: struct.pack(">fff", 100.0, 200.0, 300.0) in m); print("  Boss sees Pleb there")
# Summoned beside someone standing on the ground (their client reports
# their feet a hair below its top): beside them, not inside them.
x, y, z = boss.pos_bs; boss.pos_bs = (x, (round(y / 10 - 0.5) + 0.5) * 10 - 0.001, z); boss.playerpos(); boss.pump(0.3)
pleb.cmds.clear(); ask(boss, "/teleport Pleb Boss", "Teleporting Pleb to Boss at")
mv = struct.unpack_from(">fff", pleb.wait_for(0x34), 2); print("  Boss at", boss.pos_bs, "Pleb summoned to", mv)
assert abs(mv[0] - x) + abs(mv[2] - z) >= 9.9
ask(boss, "/teleport Pleb", "Teleporting Boss to Pleb at")
ask(boss, "/clearinv Pleb", "Cleared Pleb's inventory.")
print("ITEMS, TIME AND TELEPORT OK")

# A new password, set by the admin: the old one stops working.
ask(boss, "/setpassword Pleb s3cret", 'Password of player "Pleb" set.')
pleb.raw(0, b"\x00\x03"); time.sleep(0.5)
pleb = Client("Pleb", port); r = login(pleb, "pw"); print("  old password:", r); assert r == ("denied", "")
time.sleep(0.3)
pleb = Client("Pleb", port); r = login(pleb, "s3cret"); print("  new password:", r); assert "interact" in r
pleb.playerpos()
print("SETPASSWORD OK")

# Kicking and banning.
pleb.cmds.clear()
ask(boss, "/kick Pleb being rude", "Kicked Pleb.")
print("  Pleb was told:", denied(pleb.wait_for(0x0A)));
ask(boss, "/kick Boss", "Failed to kick player Boss.")
pleb = Client("Pleb", port); assert "interact" in login(pleb, "s3cret"); pleb.playerpos()
ask(boss, "/ban Pleb", "Banned 127.0.0.1|Pleb.")
pleb.wait_for(0x0A)
ask(boss, "/ban", "Ban list: 127.0.0.1|Pleb")
time.sleep(0.3)
pleb = Client("Pleb", port); r = login(pleb, "s3cret"); print("  banned Pleb:", r); assert r == ("denied", "Your IP is banned. Banned name was Pleb")
ask(boss, "/unban Pleb", "Unbanned Pleb.")
pleb = Client("Pleb", port); assert "interact" in login(pleb, "s3cret"); pleb.playerpos()
print("KICK AND BAN OK")

# A cleared password: the next login sets a new one, as a new account does.
ask(boss, "/clearpassword Pleb", 'Password of player "Pleb" cleared.')
pleb.raw(0, b"\x00\x03"); time.sleep(0.5)
pleb = Client("Pleb", port); p = register(pleb, "fresh"); print("  after clearing:", p)
pleb.raw(0, b"\x00\x03"); time.sleep(0.5)
pleb = Client("Pleb", port); assert "interact" in login(pleb, "fresh")
print("CLEARPASSWORD OK")

# /sethome and /home (minetest_game's sethome), with the "home" privilege.
pleb.playerpos()
ask(pleb, "/sethome", "missing privileges: home")
ask(boss, "/grant Pleb home", "Privileges of Pleb: fast, fly, home, interact, shout")
ask(pleb, "/home", "Set a home using /sethome")
ask(boss, "/teleport Pleb 5,15,-7", "Teleporting Pleb to (5.0, 15.0, -7.0).")
pleb.wait_for(0x34); pleb.pos_bs = (50.0, 150.0, -70.0); pleb.playerpos(); pleb.pump(0.3)
ask(pleb, "/sethome", "Home set!")
ask(boss, "/teleport Pleb 40,30,40", "Teleporting Pleb")
pleb.wait_for(0x34); pleb.pos_bs = (400.0, 300.0, 400.0); pleb.playerpos(); pleb.pump(0.3)
pleb.raw(0, b"\x00\x03"); time.sleep(0.5)
pleb = Client("Pleb", port); assert "home" in login(pleb, "fresh"); pleb.playerpos()
pleb.cmds.clear()
ask(pleb, "/home", "Teleported to home!")
mv = pleb.wait_for(0x34); print("  home is", struct.unpack_from(">fff", mv, 2)); assert struct.unpack_from(">fff", mv, 2) == (50.0, 150.0, -70.0)
print("SETHOME OK")

boss.raw(0, b"\x00\x03"); pleb.raw(0, b"\x00\x03"); time.sleep(0.3)
print("ADMIN OK")
