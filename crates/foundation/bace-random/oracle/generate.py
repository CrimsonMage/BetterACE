#!/usr/bin/env python3
"""Independent stdlib HMAC oracle for BetterACE algorithm-v1, synthetic key only.

This freezes our deliberate stream-isolation contract, not emulator/retail RNG
parity. Run from any working directory; no player data or server key is read.
"""
import hashlib
import hmac
import pathlib
import struct


def mac(key, data):
    return hmac.new(key, data, hashlib.sha256).digest()


def draw(key, counter):
    return struct.unpack('<Q', mac(key, b'BetterACE.random.draw.v1\0' + struct.pack('<Q', counter))[:8])[0]


rows = [
    '# Independent Python stdlib HMAC-SHA256, algorithm-v1 framing; synthetic key bytes 0..31 only.',
    '# kind,domain,ordinal,draw0,draw1,draw2,fork0',
]
for kind, domain, ordinal in [(0,1,0),(0,7,0),(1,2,123),(1,4,123),(1,6,7)]:
    identity = bytes([17 if kind else 34]) * 16
    frame = b'BetterACE.random.scope.v1\0' + struct.pack('<IBH',9,kind,domain) + identity + struct.pack('<Q',ordinal)
    key = mac(bytes(range(32)),frame)
    label = b'weapons'
    fork = mac(key,b'BetterACE.random.fork.v1\0' + struct.pack('<H',len(label)) + label + struct.pack('<Q',4))
    rows.append(','.join(map(str,[kind,domain,ordinal,*[draw(key,n) for n in range(3)],draw(fork,0)])))
path = pathlib.Path(__file__).resolve().parents[1] / 'tests/fixtures/v1.csv'
expected = '\n'.join(rows) + '\n'
if path.read_text() != expected:
    raise SystemExit('Fixture disagrees with independent HMAC oracle')
print('Algorithm-v1 fixture matches independent HMAC oracle')
