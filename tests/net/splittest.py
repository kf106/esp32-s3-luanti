import sys, time, struct, os, re
src = open(sys.argv[2]).read()
exec(src[:src.index("port = int(sys.argv[1])")])
port = int(sys.argv[1])
names = sorted(set(re.findall(r'include_bytes!\("\.\./media/([^"]+\.ogg)"\)', open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../src/content.rs")).read())))
req = struct.pack(">HH", 0x40, len(names)) + b"".join(struct.pack(">H", len(n)) + n.encode() for n in names)
print(f"requesting {len(names)} sounds in a {len(req)} byte command")
class SplitClient(Client):
    def send(s, ch, cmd):
        if cmd == struct.pack(">HH", 0x40, 0):
            # As the Luanti client does for a big request: split chunks,
            # each sent reliably. Out of order, with one sent twice.
            size = 495
            parts = [req[i:i + size] for i in range(0, len(req), size)]
            bodies = [b"\x02" + struct.pack(">HHH", 65500, len(parts), i) + p for i, p in enumerate(parts)]
            seqs = []
            for b in bodies:
                seq = s.out_seq[ch]; s.out_seq[ch] = (seq + 1) & 0xFFFF; seqs.append(seq)
            order = list(range(len(bodies)))[::-1] + [0]
            for i in order:
                s.raw(ch, b"\x03" + struct.pack(">H", seqs[i]) + bodies[i])
            return
        Client.send(s, ch, cmd)
c = SplitClient("Splitter", port); c.login()
got = set(); end = time.time() + 20
while time.time() < end and len(got) < len(names):
    for m in c.take(0x38):
        o = 2 + 4; nfiles = struct.unpack_from(">I", m, 6)[0]; o = 10
        for _ in range(nfiles):
            ln = u16(m, o); nm = m[o + 2:o + 2 + ln].decode(); o += 2 + ln
            dl = struct.unpack_from(">I", m, o)[0]; o += 4 + dl
            got.add(nm)
    c.pump(0.1)
print(f"received {len(got)} of {len(names)}"); assert got == set(names), set(names) - got
c.raw(0, b"\x00\x03"); time.sleep(0.3); print("SPLIT OK")
