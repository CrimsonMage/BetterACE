#!/usr/bin/env python3
"""Original ACE Buffs declaration and SpellId enum resolved by the C# runtime."""
from pathlib import Path
import re,tempfile,subprocess,hashlib
ROOT=Path(__file__).resolve().parents[4];SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
player=SRC/'ACE.Server/WorldObjects/Player_Spells.cs';spell=SRC/'ACE.Entity/Enum/SpellId.cs'
s=player.read_text();a=s.index('private static string[] Buffs');b=s.index('};',a)+2;declaration=s[a:b]
harness='''using System;using ACE.Entity.Enum;class Program{DECL static void Main(){for(int level=1;level<=8;level++)foreach(string branch in new[]{"Self","Other"})foreach(string buff in Buffs){bool bane=buff.StartsWith("@");string name=(bane?buff.Substring(1):buff)+(bane?"":branch)+level;Console.WriteLine(level+","+branch+","+buff+","+(uint)Enum.Parse(typeof(SpellId),name));}}}'''.replace('DECL',declaration)
with tempfile.TemporaryDirectory(prefix='staff-buffs-oracle-') as tmp:
 p=Path(tmp);(p/'SpellId.cs').write_bytes(spell.read_bytes());(p/'Program.cs').write_text(harness);(p/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
 subprocess.run(['/tmp/bace-crafting-dotnet/dotnet','build','--nologo','-o',str(p/'build')],cwd=p,check=True)
 output=subprocess.check_output(['/tmp/bace-crafting-dotnet/dotnet',str(p/'build/oracle.dll')],text=True)
 (Path(__file__).resolve().parents[1]/'tests/fixtures/staff_buffs.csv').write_text(''.join('# '+str(f.relative_to(SRC))+' '+hashlib.sha256(f.read_bytes()).hexdigest()+'\n' for f in [player,spell])+output)
