#!/usr/bin/env python3
"""Execute unmodified pinned ACE wielded generation and vendor stocking methods."""
import argparse
import hashlib
import subprocess
import tempfile
from pathlib import Path

PIN = '47edade3bd3f6044b676d4eb877c4965c7eda62b'
p = argparse.ArgumentParser()
p.add_argument('--source', type=Path, required=True)
p.add_argument('--dotnet', required=True)
a = p.parse_args()

def method(text, signature):
    start = text.index(signature)
    opening = text.index('{', start)
    depth = 0
    for i in range(opening, len(text)):
        depth += (text[i] == '{') - (text[i] == '}')
        if depth == 0:
            return text[start:i + 1]
    raise ValueError(signature)

harness = (Path(__file__).parent / 'treasure_harness.cs').read_text()
provenance = f'# Official ACEmulator/ACE {PIN}; AGPL-3.0-only; original methods, synthetic templates\n'
for filename, methods in [
    ('WorldObject_Equipment.cs', [('WIELDED', 'public static List<WorldObject> GenerateWieldedTreasureSets(List<TreasureWielded> items)'), ('WALK', 'private static void GenerateWieldedTreasureSets('), ('CREATE', 'public static WorldObject CreateWieldedTreasure(')]),
    ('WorldObject_Properties.cs', [('STACK', 'public void SetStackSize(')]),
    ('Vendor.cs', [('VENDOR', 'public void AddDefaultItem('), ('MATCH', 'public List<WorldObject> GetDefaultItemsByWcid(')]),
]:
    relative = 'Source/ACE.Server/WorldObjects/' + filename
    data = (a.source / relative).read_bytes()
    provenance += f'# sha256 {hashlib.sha256(data).hexdigest()} {relative}\n'
    for marker, signature in methods:
        harness = harness.replace('// ' + marker, method(data.decode('utf-8-sig'), signature))
with tempfile.TemporaryDirectory(prefix='bace-treasure-oracle-') as directory:
    root = Path(directory)
    (root / 'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><RollForward>Major</RollForward></PropertyGroup></Project>')
    (root / 'Program.cs').write_text(harness)
    subprocess.run([a.dotnet, 'build', '--nologo', '-o', str(root / 'out'), str(root / 'Oracle.csproj')], check=True)
    output = subprocess.check_output([a.dotnet, str(root / 'out/Oracle.dll')], text=True)
for kind, destination in [('wielded', '../tests/fixtures/wielded.csv'), ('vendor', '../../bace-economy/tests/fixtures/vendor_stock.csv')]:
    rows = [row[len(kind) + 1:] for row in output.splitlines() if row.startswith(kind + '|')]
    (Path(__file__).parent / destination).write_text(provenance + '\n'.join(rows) + '\n')
