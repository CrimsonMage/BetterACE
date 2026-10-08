#!/usr/bin/env python3
"""Run unchanged pinned Player_Spells.AuditItemSpells on synthetic inputs."""
import argparse, hashlib, subprocess, tempfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args()
rel='Source/ACE.Server/WorldObjects/Player_Spells.cs';raw=(a.source/rel).read_bytes();s=raw.decode('utf-8-sig')
start=s.index('public void AuditItemSpells()');opening=s.index('{',start);depth=0
for i in range(opening,len(s)):
 depth+=(s[i]=='{')-(s[i]=='}')
 if depth==0:method=s[start:i+1];break
source=(Path(__file__).parent/'audit_harness.cs').read_text().replace('// SOURCE_METHOD',method)
with tempfile.TemporaryDirectory(prefix='bace-audit-oracle-') as td:
 b=Path(td);(b/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>');(b/'Program.cs').write_text(source)
 subprocess.run([a.dotnet,'build','--nologo','-o',str(b/'out'),str(b/'Oracle.csproj')],check=True)
 output=subprocess.check_output([a.dotnet,str(b/'out/Oracle.dll')],text=True)
(Path(__file__).parent.parent/'tests/fixtures/audit.csv').write_text('# ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b unchanged AuditItemSpells; synthetic adapters\n# sha256 '+hashlib.sha256(raw).hexdigest()+' '+rel+'\n'+output)
