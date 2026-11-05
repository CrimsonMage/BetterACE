#!/usr/bin/env python3
"""Compile unchanged ACE housing GameAction read sequences and emit tests/fixtures/housing.csv.

Each action's `message.Payload.Read...` lines are copied verbatim from the pinned ACE
snapshot and executed against bytes written by this script's harness:
  - ReadUInt32 scalars are written as 0x1000+k, k = ordinal among scalar reads in the action;
  - ReadString16L is written as "Rune";
  - ReadListUInt32 is written as count 2 followed by items 0x2000 and 0x2001.
The row's consumed-byte count comes from ACE's own reader and is the layout oracle.
Motd (0x0254-0x0256) is decoded by group_input.rs and DoAllegianceHouseAction (0x0042) by
group_input.rs as well; both are excluded here.
"""
import hashlib
import os
import re
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
SRC = ROOT / '.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
ACTIONS = SRC / 'ACE.Server/Network/GameAction/Actions'
DOTNET = os.environ.get('BACE_DOTNET', '/home/jokerfactor/.dotnet/dotnet')

# (ACE GameActionType name, ACE action file stem)
OPCODES = [
    ('BuyHouse', 'GameActionHouseBuyHouse'),
    ('HouseQuery', 'GameActionHouseQuery'),
    ('AbandonHouse', 'GameActionHouseAbandon'),
    ('RentHouse', 'GameActionHouseRentHouse'),
    ('AddPermanentGuest', 'GameActionHouseAddPermanentGuest'),
    ('RemovePermanentGuest', 'GameActionHouseRemovePermanentGuest'),
    ('SetOpenHouseStatus', 'GameActionHouseSetOpenStatus'),
    ('ChangeStoragePermission', 'GameActionHouseChangeStoragePermission'),
    ('BootSpecificHouseGuest', 'GameActionHouseBootSpecificGuest'),
    ('RemoveAllStoragePermission', 'GameActionHouseRemoveAllStoragePermission'),
    ('RequestFullGuestList', 'GameActionHouseRequestFullGuestList'),
    ('QueryLord', 'GameActionHouseQueryLord'),
    ('AddAllStoragePermission', 'GameActionHouseAddAllStoragePermission'),
    ('RemoveAllPermanentGuests', 'GameActionHouseRemoveAllPermanentGuests'),
    ('BootEveryone', 'GameActionHouseBootEveryone'),
    ('SetHooksVisibility', 'GameActionHouseSetHooksVisibility'),
    ('ModifyAllegianceGuestPermission', 'GameActionHouseModifyAllegianceGuestPermission'),
    ('ModifyAllegianceStoragePermission', 'GameActionHouseModifyAllegianceStoragePermission'),
    ('ListAvailableHouses', 'GameActionHouseListAvailable'),
]

# Support sources needed for ReadString16L, ReadListUInt32, WriteString16L and HouseType.
SUPPORT = [
    'ACE.Common/Extensions/BinaryReaderExtensions.cs',
    'ACE.Server/Network/Extensions.cs',
    'ACE.Server/Network/Structure/PackableList.cs',
    'ACE.Entity/Enum/HouseType.cs',
]

HARNESS = '''using System;
using System.IO;
using System.Text;
using System.Collections.Generic;
using ACE.Common.Extensions;
using ACE.Entity.Enum;
using ACE.Server.Network;
using ACE.Server.Network.Structure;
namespace ACE.Server.Network { public class ClientMessage { public BinaryReader Payload; } }
namespace ACE.Server.Network.Enum { public enum StubEnum { None } }
namespace ACE.Entity { public struct ObjectGuid { public uint Full; public ObjectGuid(uint id) { Full = id; } } }
class Program {
 static void Main() {
  Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
  INPUT_CASES
 }
}
'''


def writes_for(lines):
    out = []
    scalar = 0
    for line in lines:
        if 'ReadListUInt32' in line:
            out.append('w.Write(2u);w.Write(0x2000u);w.Write(0x2001u);')
        elif 'ReadString16L' in line:
            out.append('w.WriteString16L("Rune");')
        elif 'ReadUInt32' in line:
            out.append(f'w.Write({0x1000 + scalar}u);')
            scalar += 1
        else:
            raise ValueError(f'unexpected read line: {line}')
    return ''.join(out)


def main():
    files = [SRC / rel for rel in SUPPORT]
    cases = []
    for name, stem in OPCODES:
        path = ACTIONS / f'{stem}.cs'
        source = path.read_text()
        found = re.search(r'GameActionType\.(\w+)', source).group(1)
        if found != name:
            raise SystemExit(f'{path.name} handles {found}, expected {name}')
        lines = [
            line.strip().split('//')[0].strip()
            for line in source.splitlines()
            if 'message.Payload.Read' in line
        ]
        original = '\n'.join(lines)
        cases.append(
            '{using var stream=new MemoryStream();'
            'using(var w=new BinaryWriter(stream,System.Text.Encoding.UTF8,true)){'
            + writes_for(lines)
            + '}var bytes=stream.ToArray();'
            'var message=new ClientMessage{Payload=new BinaryReader(new MemoryStream(bytes))};'
            + original
            + f'\nConsole.WriteLine("input,{name},"+Convert.ToHexString(bytes)+","'
            '+message.Payload.BaseStream.Position);}'
        )
        files.append(path)

    harness = HARNESS.replace('INPUT_CASES', '\n  '.join(cases))
    with tempfile.TemporaryDirectory(prefix='housing-oracle-') as tmp:
        p = Path(tmp)
        for i, f in enumerate(files[:len(SUPPORT)]):
            (p / f'source{i}.cs').write_bytes(f.read_bytes())
        (p / 'Program.cs').write_text(harness)
        (p / 'oracle.csproj').write_text(
            '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType>'
            '<TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings>'
            '</PropertyGroup></Project>'
        )
        subprocess.run([DOTNET, 'build', '--nologo', '-o', str(p / 'build')], cwd=p, check=True)
        output = subprocess.check_output([DOTNET, str(p / 'build/oracle.dll')], text=True)
        header = ''.join(
            '# ' + str(f.relative_to(SRC)) + ' ' + hashlib.sha256(f.read_bytes()).hexdigest() + '\n'
            for f in files
        )
        target = Path(__file__).resolve().parents[1] / 'tests/fixtures/housing.csv'
        target.write_text(header + output)
        print(target)


if __name__ == '__main__':
    main()
