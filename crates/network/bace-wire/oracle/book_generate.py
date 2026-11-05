#!/usr/bin/env python3
"""Compile unchanged ACE book and inscription GameAction read sequences and emit tests/fixtures/book.csv.

Each action's `message.Payload.Read...` lines are copied verbatim from the pinned ACE
snapshot and executed against bytes written by this script's harness:
  - ReadUInt32 scalars are written as 0x1000+k, k = ordinal among ReadUInt32 reads in the action;
  - ReadInt32 (page index) is written as 3;
  - ReadString16L is written as "Rune" (ASCII, so ACE's UTF-8 reader and the CP1252 reader agree).
The row's consumed-byte count comes from ACE's own reader and is the layout oracle.
Non-ASCII text, where retail and ACE's reader disagree, is covered in tests/book_vectors.rs
from the retail sender CM_Writing.cpp and AC1Legacy__PStringBase.cpp, not from this oracle.
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
    ('BookData', 'GameActionBookData'),
    ('BookModifyPage', 'GameActionBookModifyPage'),
    ('BookAddPage', 'GameActionBookAddPage'),
    ('BookDeletePage', 'GameActionBookDeletePage'),
    ('BookPageData', 'GameActionBookPageData'),
    ('SetInscription', 'GameActionSetInscription'),
]

# Support sources needed for ReadString16L and WriteString16L.
SUPPORT = [
    'ACE.Common/Extensions/BinaryReaderExtensions.cs',
    'ACE.Server/Network/Extensions.cs',
]

HARNESS = '''using System;
using System.IO;
using System.Text;
using System.Collections.Generic;
using ACE.Common.Extensions;
using ACE.Server.Network;
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
        if 'ReadString16L' in line:
            out.append('w.WriteString16L("Rune");')
        elif 'ReadInt32' in line:
            out.append('w.Write(3);')
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
    with tempfile.TemporaryDirectory(prefix='book-oracle-') as tmp:
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
        target = Path(__file__).resolve().parents[1] / 'tests/fixtures/book.csv'
        target.write_text(header + output)
        print(target)


if __name__ == '__main__':
    main()
