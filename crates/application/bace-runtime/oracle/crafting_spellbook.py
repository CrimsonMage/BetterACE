#!/usr/bin/env python3
"""Execute pinned ACE GetOrAddKnownSpell for crafting persistence fixtures.
ACEmulator/ACE contributors, AGPL-3.0-only. The method is compiled unchanged;
Biota's dictionary and lock are minimal surrounding storage adapters.
"""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile
p = argparse.ArgumentParser()
p.add_argument('--repo', type=Path, required=True)
p.add_argument('--dotnet', required=True)
a = p.parse_args()
pin = '47edade3bd3f6044b676d4eb877c4965c7eda62b'
path = 'Source/ACE.Entity/Models/BiotaExtensions.cs'
source = subprocess.check_output(['git', '-C', str(a.repo), 'show', f'{pin}:{path}'])
s = source.decode(); start = s.index('public static float GetOrAddKnownSpell('); opening = s.index('{', start); depth = 0
for end in range(opening, len(s)):
    depth += (s[end] == '{') - (s[end] == '}')
    if depth == 0:
        method = s[start:end+1]
        break
harness = '''using System; using System.Collections.Generic; using System.Threading;
class Biota {public Dictionary<int,float> PropertiesSpellBook;}
static class Extensions { METHOD }
class Program {static void Main(){foreach(float? old in new float?[]{null,0f,0.125f,1f,2f}) {
 var b=new Biota();if(old.HasValue)b.PropertiesSpellBook=new(){{7,old.Value}};
 using var rw=new ReaderWriterLockSlim();var value=b.GetOrAddKnownSpell(7,rw,out var added);
 Console.WriteLine($"{(old.HasValue?BitConverter.SingleToUInt32Bits(old.Value).ToString():"absent")},{BitConverter.SingleToUInt32Bits(value)},{(added?1:0)},{b.PropertiesSpellBook.Count}");
}}}
'''.replace('METHOD', method)
with tempfile.TemporaryDirectory(prefix='bace-craft-book-') as tmp:
    tmp = Path(tmp)
    (tmp/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
    (tmp/'Program.cs').write_text(harness)
    subprocess.run([a.dotnet,'build',str(tmp/'Oracle.csproj'),'--nologo','-v:q'],check=True)
    result = subprocess.check_output([a.dotnet,str(tmp/'bin/Debug/net8.0/Oracle.dll')])
    assert len(result.splitlines()) == 5
    output = Path(__file__).resolve().parents[1]/'tests/fixtures/crafting_spellbook.csv'
    output.write_bytes(f'# ACE {pin} {path} SHA256 {hashlib.sha256(source).hexdigest()}\n'.encode()+result)
