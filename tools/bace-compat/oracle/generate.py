#!/usr/bin/env python3
"""Generate vectors by compiling UNMODIFIED source from the official ACE pin.
Usage: generate.py --source .reference/ACE-<pin> --dotnet /path/to/dotnet
Only named fixture output is overwritten. Builds run in a temporary directory.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import urllib.request

PIN = '47edade3bd3f6044b676d4eb877c4965c7eda62b'
SOURCES = {
 'Source/ACE.Server/Network/PacketHeader.cs': '0312b4b4f68732a8f8fe06d7f69663ef103bd98667ed73bdcf3923349ffc0c75',
 'Source/ACE.Server/Network/PacketFragmentHeader.cs': 'affeb08de165fc49549be260341790b39309c4ca43685e846cac9e5362665342',
 'Source/ACE.Common/Cryptography/Hash32.cs': 'cbb79d9a1a4c97e3f978fb6e748eda4dfc943398b4f0faa937179ec8d7b3d985',
 'Source/ACE.Common/Cryptography/ISAAC.cs': 'fb58de63a1fc5cc852de6fda04851c901f08f932ba0d330ee4097eb6fac4f5a2',
}
# Additional upstream dependencies are verified against the pinned Git blob, too.
EXTRA = ['Source/ACE.Server/Network/' + name for name in ['PacketHeaderFlags.cs', 'PacketHeaderFlagsUtil.cs', 'Packet.cs', 'PacketFragment.cs', 'ServerPacket.cs', 'ServerPacketFragment.cs', 'PacketHeaderOptional.cs', 'Extensions.cs', 'Enum/SessionState.cs', 'Packets/PacketoutboundConnectRequest.cs', 'Packets/PacketInboundLoginRequest.cs', 'Enum/NetAuthType.cs']] + ['Source/ACE.Entity/ObjectGuid.cs', 'Source/ACE.Entity/Enum/AuthFlags.cs', 'Source/ACE.Common/Extensions/BinaryReaderExtensions.cs']
EXTRA += ['Source/ACE.Server/Network/Sequence/' + name for name in ['ISequence.cs', 'ByteSequence.cs', 'UShortSequence.cs']]
parser = argparse.ArgumentParser()
parser.add_argument('--source', type=Path)
parser.add_argument('--dotnet', default='dotnet')
args = parser.parse_args()
root = Path(__file__).resolve().parent
with tempfile.TemporaryDirectory(prefix='ace-official-oracle-') as tmp:
    build = Path(tmp)
    hashes = {}
    for path in list(SOURCES) + EXTRA:
        # All inputs, including local checkouts, are validated against immutable GitHub bytes.
        official = urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{path}', timeout=30).read()
        data = (args.source / path).read_bytes() if args.source else official
        if data != official:
            raise SystemExit(f'Not pinned upstream source: {path}')
        digest = hashlib.sha256(data).hexdigest()
        if path in SOURCES and digest != SOURCES[path]:
            raise SystemExit(f'SHA mismatch: {path}')
        hashes[path] = digest
        (build / Path(path).name).write_bytes(data)
    (build / 'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework><CheckForOverflowUnderflow>false</CheckForOverflowUnderflow></PropertyGroup></Project>')
    (build / 'Program.cs').write_bytes((root / 'Program.cs').read_bytes())
    subprocess.run([args.dotnet, 'build', '--nologo', '-o', str(build / 'out'), str(build / 'Oracle.csproj')], check=True)
    result = subprocess.check_output([args.dotnet, str(build / 'out/Oracle.dll')], text=True)
    fixtures = {'repository': 'https://github.com/ACEmulator/ACE', 'commit': PIN, 'source_sha256': hashes, 'vectors': json.loads(result)}
    (root.parent / 'fixtures/primitives.json').write_text(json.dumps(fixtures, indent=2) + '\n')
