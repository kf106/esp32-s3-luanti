"""Client side of Luanti's SRP-6a (src/util/srp.cpp + src/client/client.cpp), with Python ints.
  register NAME PASSWORD           -> salt_hex verifier_hex
  start A_SECRET_HEX               -> A_hex
  proof NAME PASSWORD SALT A_SECRET B -> M_hex
"""
import hashlib, sys
N = int("AC6BDB41324A9A9BF166DE5E1389582FAF72B6651987EE07FC3192943DB56050A37329CBB4A099ED8193E0757767A13DD52312AB4B03310DCD7F48A9DA04FD50E8083969EDB767B0CF6095179A163AB3661A05FBD5FAAAE82918A9962F0B93B855F97993EC975EEAA80D740ADBF4FF747359D041D5C33EA71D281E446B14773BCA97B43A23FB801676BD207A436C6481F1D2B9078717461A5B9D32E688F87748544523B524B0D57D5EA77A2775D2ECFA032CFBDBF52FB3786160279004E57AE6AF874E7303CE53299CCC041C7BC308D82A5698F3A8D0C38271AE35F8E9DBFBB694B5C803D89F7AE435DE236D525F54759B65E372FCD68EF20FA7111F9E4AFF73", 16)
g = 2
LN = 256
H = lambda b: hashlib.sha256(b).digest()
mb = lambda n: n.to_bytes((n.bit_length() + 7) // 8, "big")   # mpz_to_bin
pad = lambda n: n.to_bytes(LN, "big")
i = lambda b: int.from_bytes(b, "big")

def x_of(salt, name, pw): return i(H(salt + H(name.lower().encode() + b":" + pw.encode())))

cmd = sys.argv[1]
if cmd == "register":
    name, pw, salt = sys.argv[2], sys.argv[3], bytes.fromhex(sys.argv[4])
    print(salt.hex(), mb(pow(g, x_of(salt, name, pw), N)).hex())
elif cmd == "start":
    print(mb(pow(g, i(bytes.fromhex(sys.argv[2])), N)).hex())
elif cmd == "proof":
    name, pw, salt, a, B = sys.argv[2], sys.argv[3], bytes.fromhex(sys.argv[4]), i(bytes.fromhex(sys.argv[5])), i(bytes.fromhex(sys.argv[6]))
    A = pow(g, a, N)
    u = i(H(pad(A) + pad(B)))
    x = x_of(salt, name, pw)
    k = i(H(pad(N) + pad(g)))
    assert B != 0 and u != 0
    S = pow((B - k * pow(g, x, N)) % N, a + u * x, N)
    K = H(mb(S))
    hx = bytes(p ^ q for p, q in zip(H(mb(N)), H(mb(g))))
    print(H(hx + H(name.encode()) + salt + mb(A) + mb(B) + K).hex())
