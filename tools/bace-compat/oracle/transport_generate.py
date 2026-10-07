#!/usr/bin/env python3
"""Compile pinned, byte-verified ACE transport methods without rewriting logic.

Usage: transport_generate.py --source .reference/ACE-<pin> --dotnet /path/to/dotnet
Synthetic dependencies in transport_harness.cs supply clocks, logging, input
messages and output capture only. Extracted methods are included verbatim, and
both whole-file and extracted-method SHA-256 values are recorded in the fixture.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import urllib.request

PIN = '47edade3bd3f6044b676d4eb877c4965c7eda62b'
NET = 'Source/ACE.Server/Network/'
FILES = [NET + name + '.cs' for name in [
    'PacketHeader', 'PacketHeaderFlags', 'PacketHeaderFlagsUtil',
    'PacketFragmentHeader', 'Packet', 'PacketFragment', 'ServerPacket',
    'ServerPacketFragment', 'MessageFragment', 'NetworkBundle', 'GameMessageGroup',
    'Sequence/UIntSequence', 'Sequence/ISequence', 'Packets/PacketRejectRetransmit',
    'Packets/PacketoutboundConnectRequest',
    'NetworkSession',
]] + ['Source/ACE.Common/Cryptography/' + name + '.cs' for name in ['Hash32', 'ISAAC']]
METHODS = ['SendBundle', 'WriteOptionalHeaders', 'AcknowledgeSequence',
           'Retransmit', 'FlushPackets', 'SendPacket', 'DoRequestForRetransmission',
           'PruneCachedPackets']


def extract(source, method):
    # Start at an exact private method declaration and retain its original bytes.
    import re
    match = re.search(r'^        private (?:void|bool) ' + method + r'\([^\n]*\)', source, re.M)
    if not match:
        raise ValueError('method declaration missing: ' + method)
    opening = source.index('{', match.start())
    depth = 0
    for position in range(opening, len(source)):
        if source[position] == '{':
            depth += 1
        elif source[position] == '}':
            depth -= 1
            if depth == 0:
                return source[match.start():position + 1]
    raise ValueError('unterminated method: ' + method)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--source', type=Path)
    parser.add_argument('--dotnet', default='dotnet')
    args = parser.parse_args()
    root = Path(__file__).resolve().parent

    def verified(path):
        official = urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{path}', timeout=30).read()
        data = (args.source / path).read_bytes() if args.source else official
        if data != official:
            raise ValueError('not pinned upstream bytes: ' + path)
        return path, data

    with ThreadPoolExecutor(max_workers=4) as workers:
        sources = dict(workers.map(verified, FILES))
    hashes = {path: hashlib.sha256(data).hexdigest() for path, data in sources.items()}
    methods = {method: extract(sources[NET + 'NetworkSession.cs'].decode(), method) for method in METHODS}
    with tempfile.TemporaryDirectory(prefix='ace-transport-oracle-') as temporary:
        build = Path(temporary)
        for path, data in sources.items():
            if path != NET + 'NetworkSession.cs':
                (build / Path(path).name).write_bytes(data)
        harness = (root / 'transport_harness.cs').read_text()
        assert harness.count('/* PINNED_METHODS */') == 1
        (build / 'Harness.cs').write_text(harness.replace('/* PINNED_METHODS */', '\n\n'.join(methods.values())))
        (build / 'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework><CheckForOverflowUnderflow>false</CheckForOverflowUnderflow></PropertyGroup></Project>')
        subprocess.run([args.dotnet, 'build', '--nologo', '-o', str(build / 'out'), str(build / 'Oracle.csproj')], check=True)
        result = subprocess.check_output([args.dotnet, str(build / 'out/Oracle.dll')], text=True)
    fixture = {
        'repository': 'https://github.com/ACEmulator/ACE', 'commit': PIN,
        'source_sha256': hashes,
        'extracted_method_sha256': {name: hashlib.sha256(text.encode()).hexdigest() for name, text in methods.items()},
        'harness_sha256': hashlib.sha256((root / 'transport_harness.cs').read_bytes()).hexdigest(),
        'harness': 'Synthetic clocks, logging, input streams, session identity and byte capture. Algorithms are verbatim pinned methods; complete NetworkSession lifecycle is not executed.',
        'vectors': json.loads(result),
    }
    (root.parent / 'fixtures/transport.json').write_text(json.dumps(fixture, indent=2) + '\n')


if __name__ == '__main__':
    main()
