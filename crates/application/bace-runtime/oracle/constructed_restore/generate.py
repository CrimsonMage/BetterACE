#!/usr/bin/env python3
"""Run unchanged pinned ACE constructor/equipment methods against tracing helpers."""
from pathlib import Path
import hashlib, json, subprocess, sys, tempfile
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
PIN = '47edade3bd3f6044b676d4eb877c4965c7eda62b'
SOURCE = ROOT / f'.reference/ACE-{PIN}/Source/ACE.Server/WorldObjects'
METHODS = {
    'Creature.cs': ['public Creature(Weenie weenie, ObjectGuid guid)', 'public Creature(Biota biota)', 'private void InitializePropertyDictionaries()', 'private void SetEphemeralValues()'],
    'Creature_Equipment.cs': ['public bool TryWieldObject(WorldObject worldObject, EquipMask wieldedLocation)', 'private void TryActivateItemSpells(WorldObject item)'],
    'Monster_Inventory.cs': ['public void EquipInventoryItems(bool weaponsOnly = false)'],
    'Container.cs': ['private void SetEphemeralValues(bool fromBiota)'],
}
def extract(text, signature):
    start = text.index(signature)
    end = text.index('{', start) + 1
    depth = 1
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    return text[start:end]
code = (HERE / 'Harness.cs').read_text()
manifest = {}
for filename, signatures in METHODS.items():
    source = (SOURCE / filename).read_text()
    extracted = '\n'.join(extract(source, signature) for signature in signatures)
    marker = 'CONTAINER_METHODS' if filename == 'Container.cs' else filename.replace('.', '_').upper()
    code = code.replace(f'/* {marker} */', extracted)
    manifest[filename] = {'sha256': hashlib.sha256((SOURCE / filename).read_bytes()).hexdigest(), 'methods': signatures,
                          'extracted_sha256': hashlib.sha256(extracted.encode()).hexdigest()}
with tempfile.TemporaryDirectory(prefix='bace-constructed-restore-oracle-') as temporary:
    target = Path(temporary)
    (target / 'Program.cs').write_text(code)
    (target / 'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><Nullable>disable</Nullable></PropertyGroup></Project>')
    dotnet = sys.argv[1] if len(sys.argv)>1 else '/tmp/bace-crafting-dotnet/dotnet'
    build = subprocess.run([dotnet, 'build', '--nologo', '-o', str(target / 'out')], cwd=target, capture_output=True, text=True)
    if build.returncode:
        print(build.stdout); print(build.stderr); build.check_returncode()
    result = subprocess.run([dotnet,str(target / 'out/Oracle.dll')], cwd=target,capture_output=True,text=True,check=True)
    rows = json.loads(result.stdout)
    golden = {'upstream':'ACEmulator/ACE','pin':PIN,'sources':manifest,'cases':rows}
    (HERE / 'golden.json').write_text(json.dumps(golden,indent=2)+'\n')
    print(f'{len(rows)} source constructor cases -> {HERE / "golden.json"}')
