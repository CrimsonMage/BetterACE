#!/usr/bin/env python3
"""Compile unmodified pinned ACE DAT decoders against synthetic record bytes.
No proprietary archive, record, or captured player bytes enter these fixtures.
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
BASE = 'Source/ACE.DatLoader/'
FILES = [BASE + name + '.cs' for name in [
    'IUnpackable', 'BinaryReaderExtensions', 'UnpackableExtensions', 'DatFileType',
    'DatFileTypeAttribute', 'DatDatabaseType', 'DatDatabaseTypeAttribute',
    'DatFileTypeExtensionAttribute', 'DatFileTypeIdRangeAttribute',
    'FileTypes/FileType', 'FileTypes/XpTable', 'FileTypes/SkillTable', 'FileTypes/CharGen',
    'Entity/SkillBase', 'Entity/SkillFormula', 'Entity/StarterArea', 'Entity/HeritageGroupCG',
    'Entity/SkillCG', 'Entity/TemplateCG', 'Entity/SexCG', 'Entity/Position', 'Entity/Frame',
    'Entity/ObjDesc', 'Entity/HairStyleCG', 'Entity/EyeStripCG', 'Entity/FaceStripCG', 'Entity/GearCG',
    'Entity/SubPalette', 'Entity/TextureMapChange', 'Entity/AnimationPartChange',
]] + ['Source/ACE.Entity/Enum/' + name + '.cs' for name in ['Skill', 'Properties/PropertyAttribute', 'AttributeExtensions']]


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
    with tempfile.TemporaryDirectory(prefix='ace-dat-oracle-') as temporary:
        build = Path(temporary)
        for index, (path, data) in enumerate(sources.items()):
            (build / (str(index) + '_' + Path(path).name)).write_bytes(data)
        (build / 'Program.cs').write_bytes((root / 'dat_harness.cs').read_bytes())
        (build / 'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework><CheckForOverflowUnderflow>false</CheckForOverflowUnderflow></PropertyGroup></Project>')
        subprocess.run([args.dotnet, 'build', '--nologo', '-o', str(build / 'out'), str(build / 'Oracle.csproj')], check=True)
        output = subprocess.check_output([args.dotnet, str(build / 'out/Oracle.dll')], text=True)
    fixtures = {
        'repository': 'https://github.com/ACEmulator/ACE', 'commit': PIN,
        'source_sha256': {path: hashlib.sha256(data).hexdigest() for path, data in sources.items()},
        'harness_sha256': hashlib.sha256((root / 'dat_harness.cs').read_bytes()).hexdigest(),
        'harness': 'Synthetic BinaryWriter record construction; all table and nested decoders are unchanged official sources. Unused Frame convenience-constructor dependency ACE.Entity.Position is a synthetic stub.',
        'vectors': json.loads(output),
    }
    (root.parent / 'fixtures/dat.json').write_text(json.dumps(fixtures, indent=2) + '\n')


if __name__ == '__main__':
    main()
