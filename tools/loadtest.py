#!/usr/bin/env python3
"""Load test for the ESP32 Luanti server: several bot players log in and
walk off in different directions, so the server has to generate and send
new map blocks for each of them, as real players exploring would.

    python3 tools/loadtest.py 192.168.68.137 --bots 5 --minutes 10

Each bot is a minimal Luanti 5.17 client (protocol 53): it logs in with
SRP (registering an account the first time, password "loadtest"), skips
the media download, then walks in a straight line at walking speed,
sending its position ten times a second. Every few seconds the script
prints, per bot, how far it has gone, how many map blocks it has received
and how fast, its round trip time (median), its longest silence from the
server while walking, and how many of its packets it had to resend. A bot that hears nothing for 30 s counts as lost.

Bots are named Bot1, Bot2 and so on, and the server saves them like any
player: next time they start where they stopped, in fresh terrain. The
server allows six players at once, real ones included.

Needs only Python 3.8 or later.
"""
import argparse, hashlib, math, os, random, socket, struct, sys, threading, time

PROTO_ID = 0x4F457403
SEQ0 = 65500
# SRP group (RFC 5054, 2048 bits), as Luanti uses.
N = int(
    "AC6BDB41324A9A9BF166DE5E1389582FAF72B6651987EE07FC3192943DB56050A37329CBB4A099ED8193E0757767A13DD52312AB4B0331"
    "0DCD7F48A9DA04FD50E8083969EDB767B0CF6095179A163AB3661A05FBD5FAAAE82918A9962F0B93B855F97993EC975EEAA80D740ADBF4F"
    "F747359D041D5C33EA71D281E446B14773BCA97B43A23FB801676BD207A436C6481F1D2B9078717461A5B9D32E688F87748544523B524B0D"
    "57D5EA77A2775D2ECFA032CFBDBF52FB3786160279004E57AE6AF874E7303CE53299CCC041C7BC308D82A5698F3A8D0C38271AE35F8E9DB"
    "FBB694B5C803D89F7AE435DE236D525F54759B65E372FCD68EF20FA7111F9E4AFF73", 16)

TOCLIENT_HELLO = 0x02
TOCLIENT_AUTH_ACCEPT = 0x03
TOCLIENT_ACCESS_DENIED = 0x0A
TOCLIENT_BLOCKDATA = 0x20
TOCLIENT_MOVE_PLAYER = 0x34
TOCLIENT_ANNOUNCE_MEDIA = 0x3C
TOCLIENT_SRP_BYTES_S_B = 0x60

LOST_AFTER = 30.0  # seconds without a word from the server
RESEND_AFTER = 0.5


def u16(b, o=0):
    return struct.unpack_from(">H", b, o)[0]


def sha(b):
    return hashlib.sha256(b).digest()


def mpz(n):
    return n.to_bytes((n.bit_length() + 7) // 8, "big")


def pad(n):
    return n.to_bytes(256, "big")


def num(b):
    return int.from_bytes(b, "big")


class Denied(Exception):
    pass


class Bot:
    def __init__(self, name, host, port, password, heading, speed, view):
        self.name, self.addr, self.password = name, (host, port), password
        self.heading, self.speed, self.view = heading, speed, view
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.sock.settimeout(0.02)
        self.peer = 0
        self.out_seq = [SEQ0] * 3
        self.in_seq = [SEQ0] * 3
        self.early = [dict() for _ in range(3)]
        self.splits = {}
        self.cmds = []
        self.unacked = {}  # (channel, seqnum) -> [body, first sent, last sent, tries]
        self.lock = threading.Lock()
        # Figures for the reports.
        self.state = "starting"
        self.blocks = 0
        self.blocks_reported = 0
        self.rtts = []
        self.resends = 0
        self.last_heard = time.time()
        self.longest_gap = 0.0
        self.walked = 0.0
        self.login_took = None
        self.pos = (0.0, 0.0, 0.0)

    # --- packets -------------------------------------------------------

    def raw(self, ch, body):
        try:
            self.sock.sendto(struct.pack(">IHB", PROTO_ID, self.peer, ch) + body, self.addr)
        except OSError:
            pass

    def send(self, ch, cmd):
        """Send a command reliably: it is resent until acknowledged."""
        seq = self.out_seq[ch]
        self.out_seq[ch] = (seq + 1) & 0xFFFF
        body = b"\x03" + struct.pack(">H", seq) + b"\x01" + cmd
        now = time.time()
        self.unacked[(ch, seq)] = [body, now, now, 0]
        self.raw(ch, body)

    def send_unreliable(self, ch, cmd):
        self.raw(ch, b"\x01" + cmd)

    def resend(self):
        now = time.time()
        for key, u in self.unacked.items():
            if now - u[2] > RESEND_AFTER * 1.5 ** min(u[3], 6):
                self.raw(key[0], u[0])
                u[2] = now
                u[3] += 1
                self.resends += 1

    def handle_inner(self, ch, p):
        t = p[0]
        if t == 0:  # control
            if p[1] == 0:  # ACK
                u = self.unacked.pop((ch, u16(p, 2)), None)
                if u and u[3] == 0:
                    self.rtts.append(time.time() - u[1])
            elif p[1] == 1:  # SET_PEER_ID
                self.peer = u16(p, 2)
        elif t == 1:  # original
            self.cmds.append(p[1:])
        elif t == 2:  # split
            seq, count, n = struct.unpack_from(">HHH", p, 1)
            parts = self.splits.setdefault((ch, seq), {})
            parts[n] = p[7:]
            if len(parts) == count:
                self.cmds.append(b"".join(parts[i] for i in range(count)))
                del self.splits[(ch, seq)]

    def pump(self, secs):
        end = time.time() + secs
        while True:
            try:
                d, _ = self.sock.recvfrom(4096)
            except socket.timeout:
                if time.time() >= end:
                    break
                continue
            except OSError:
                break
            now = time.time()
            self.longest_gap = max(self.longest_gap, now - self.last_heard)
            self.last_heard = now
            if len(d) < 8 or struct.unpack_from(">I", d)[0] != PROTO_ID:
                continue
            ch, p = d[6], d[7:]
            if ch > 2 or not p:
                continue
            if p[0] == 3:  # reliable
                seq = u16(p, 1)
                self.raw(ch, b"\x00\x00" + struct.pack(">H", seq))
                ahead = (seq - self.in_seq[ch]) & 0xFFFF
                if ahead == 0:
                    self.handle_inner(ch, p[3:])
                    self.in_seq[ch] = (self.in_seq[ch] + 1) & 0xFFFF
                    while self.in_seq[ch] in self.early[ch]:
                        self.handle_inner(ch, self.early[ch].pop(self.in_seq[ch]))
                        self.in_seq[ch] = (self.in_seq[ch] + 1) & 0xFFFF
                elif ahead < 0x8000:
                    self.early[ch][seq] = p[3:]
            else:
                self.handle_inner(ch, p)
            if time.time() >= end:
                break
        self.resend()

    def take(self, cmd_id):
        got = [c for c in self.cmds if len(c) >= 2 and u16(c) == cmd_id]
        self.cmds = [c for c in self.cmds if not (len(c) >= 2 and u16(c) == cmd_id)]
        return got

    def wait_for(self, cmd_id, secs=30):
        end = time.time() + secs
        while time.time() < end:
            for c in self.take(TOCLIENT_ACCESS_DENIED):
                raise Denied(denial(c))
            got = self.take(cmd_id)
            if got:
                return got[0]
            self.pump(0.05)
        raise TimeoutError(f"no reply {cmd_id:#04x} from the server within {secs} s")

    # --- logging in ----------------------------------------------------

    def login(self):
        started = self.last_heard = time.time()
        self.state = "login"
        name = self.name.encode()
        self.send(0, struct.pack(">HBHHH", 0x02, 29, 0, 37, 53) + struct.pack(">H", len(name)) + name)
        hello = self.wait_for(TOCLIENT_HELLO)
        mechs = struct.unpack_from(">I", hello, 7)[0]
        x_of = lambda salt: num(sha(salt + sha(self.name.lower().encode() + b":" + self.password.encode())))
        if mechs & 4:  # FIRST_SRP: a new account
            salt = os.urandom(16)
            v = mpz(pow(2, x_of(salt), N))
            self.send(0, struct.pack(">HH", 0x50, 16) + salt + struct.pack(">H", len(v)) + v + b"\x00")
        elif mechs & 2:  # SRP: log in to an existing account
            a = num(os.urandom(32))
            A = pow(2, a, N)
            self.send(0, struct.pack(">HH", 0x51, len(mpz(A))) + mpz(A) + b"\x01")
            sb = self.wait_for(TOCLIENT_SRP_BYTES_S_B)
            o = 2
            ls = u16(sb, o)
            salt = sb[o + 2:o + 2 + ls]
            o += 2 + ls
            lb = u16(sb, o)
            B = num(sb[o + 2:o + 2 + lb])
            u = num(sha(pad(A) + pad(B)))
            k = num(sha(pad(N) + pad(2)))
            x = x_of(salt)
            S = pow((B - k * pow(2, x, N)) % N, a + u * x, N)
            K = sha(mpz(S))
            hx = bytes(p ^ q for p, q in zip(sha(mpz(N)), sha(mpz(2))))
            M = sha(hx + sha(name) + salt + mpz(A) + mpz(B) + K)
            self.send(0, struct.pack(">HH", 0x52, len(M)) + M)
        else:
            raise Denied(f"no SRP login offered (mechanisms {mechs:#x})")
        self.wait_for(TOCLIENT_AUTH_ACCEPT)
        self.state = "loading"
        self.send(1, struct.pack(">HH", 0x11, 0))  # INIT2, language ""
        self.wait_for(TOCLIENT_ANNOUNCE_MEDIA)
        self.send(1, struct.pack(">HH", 0x40, 0))  # REQUEST_MEDIA: nothing (cached)
        ver = b"5.17.0"
        self.send(1, struct.pack(">HBBBB", 0x43, 5, 17, 0, 0) + struct.pack(">H", len(ver)) + ver + struct.pack(">H", 11))
        mv = self.wait_for(TOCLIENT_MOVE_PLAYER)
        self.pos = tuple(v / 10 for v in struct.unpack_from(">fff", mv, 2))
        self.login_took = time.time() - started
        self.cmds.clear()

    # --- walking -------------------------------------------------------

    def send_position(self, moving):
        # The server takes the look direction from yaw: 0 looks along +Z,
        # and the walking direction is (cos(yaw + 90), sin(yaw + 90)).
        yaw = math.degrees(self.heading) - 90.0
        vx = math.cos(self.heading) * self.speed if moving else 0.0
        vz = math.sin(self.heading) * self.speed if moving else 0.0
        p = [int(v * 1000) for v in self.pos]  # BS units x 100
        keys = 1 if moving else 0  # "up"
        self.send_unreliable(0, struct.pack(">H3i3iiiIBB", 0x23, *p, int(vx * 1000), 0, int(vz * 1000),
                                            0, int(yaw * 100), keys, 72, self.view))

    def run(self, until, stop):
        try:
            self.login()
        except Denied as e:
            self.state = f"turned away: {e}"
            return
        except (TimeoutError, OSError) as e:
            self.state = f"login failed: {e}"
            return
        self.state = "walking"
        # The longest silence counts from here: logging in has its own figure.
        self.longest_gap = 0.0
        self.last_heard = last = time.time()
        while time.time() < until and not stop.is_set():
            now = time.time()
            dt = now - last
            last = now
            x, y, z = self.pos
            step = self.speed * dt
            self.pos = (x + math.cos(self.heading) * step, y, z + math.sin(self.heading) * step)
            self.walked += step
            self.send_position(True)
            self.pump(0.1)
            for c in self.take(TOCLIENT_ACCESS_DENIED):
                self.state = f"kicked: {denial(c)}"
                return
            self.blocks += len(self.take(TOCLIENT_BLOCKDATA))
            self.cmds.clear()
            if time.time() - self.last_heard > LOST_AFTER:
                self.state = "lost (server silent 30 s)"
                return
        self.send_position(False)
        self.raw(0, b"\x00\x03")  # DISCO
        self.state = "done"


def denial(cmd):
    reasons = {0: "wrong password", 1: "unexpected data", 2: "single-player", 3: "wrong version",
               4: "bad name characters", 5: "bad name", 6: "too many users", 7: "empty password",
               8: "already connected", 9: "server failure", 10: "custom", 11: "shutdown", 12: "crash"}
    return reasons.get(cmd[2] if len(cmd) > 2 else -1, "denied")


def report(bots, started, interval, final=False):
    elapsed = time.time() - started
    print(f"\n{'Final' if final else 'At'} {elapsed:5.0f} s")
    print(f"  {'bot':<7}{'state':<12}{'walked':>8}{'blocks':>8}{'blk/s':>7}{'rtt ms':>8}{'silence':>9}{'resent':>8}")
    total_rate = 0.0
    for b in bots:
        rate = (b.blocks - b.blocks_reported) / interval if not final else b.blocks / max(elapsed, 1)
        b.blocks_reported = b.blocks
        total_rate += rate
        rtts = sorted(b.rtts[-200:])
        rtt = f"{rtts[len(rtts) // 2] * 1000:.0f}" if rtts else "-"
        state = b.state if len(b.state) <= 11 else b.state[:11]
        print(f"  {b.name:<7}{state:<12}{b.walked:7.0f}m{b.blocks:8d}{rate:7.1f}{rtt:>8}{b.longest_gap:8.1f}s{b.resends:8d}")
        if len(b.state) > 11:
            print(f"         {b.state}")
    print(f"  all bots: {total_rate:.1f} blocks/s"
          + ("" if final else f"; logins took {', '.join(f'{b.login_took:.1f}' for b in bots if b.login_took)} s"))


def main():
    ap = argparse.ArgumentParser(description="Bot players that walk off in different directions.")
    ap.add_argument("host", help="the server's address, as shown on its screen")
    ap.add_argument("--port", type=int, default=30000)
    ap.add_argument("--bots", type=int, default=3, help="how many bots (the server takes 6 players)")
    ap.add_argument("--minutes", type=float, default=5.0, help="how long they walk")
    ap.add_argument("--speed", type=float, default=4.0, help="nodes per second (Luanti walks at 4)")
    ap.add_argument("--stagger", type=float, default=3.0, help="seconds between bots joining")
    ap.add_argument("--report", type=float, default=10.0, help="seconds between reports")
    ap.add_argument("--view", type=int, default=10, help="view range asked for, in map blocks")
    ap.add_argument("--prefix", default="Bot", help="bot names: <prefix>1, <prefix>2, ...")
    ap.add_argument("--password", default="loadtest")
    args = ap.parse_args()

    start_angle = random.uniform(0, 2 * math.pi)
    bots = [Bot(f"{args.prefix}{i + 1}", args.host, args.port, args.password,
                start_angle + 2 * math.pi * i / args.bots, args.speed, args.view) for i in range(args.bots)]
    stop = threading.Event()
    started = time.time()
    until = started + args.stagger * args.bots + args.minutes * 60
    threads = []
    for b in bots:
        t = threading.Thread(target=b.run, args=(until, stop), daemon=True)
        t.start()
        threads.append(t)
        print(f"{b.name} heading {math.degrees(b.heading) % 360:.0f} degrees")
        time.sleep(args.stagger)
    try:
        next_report = time.time() + args.report
        while any(t.is_alive() for t in threads):
            time.sleep(0.2)
            if time.time() >= next_report:
                report(bots, started, args.report)
                next_report += args.report
    except KeyboardInterrupt:
        print("\nStopping the bots...")
        stop.set()
        for t in threads:
            t.join(2)
    report(bots, started, args.report, final=True)
    lost = [b.name for b in bots if b.state.startswith(("lost", "kicked", "login failed"))]
    if lost:
        print(f"Trouble: {', '.join(lost)}")
        sys.exit(1)


if __name__ == "__main__":
    main()
