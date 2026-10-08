#!/usr/bin/env python3
"""Original material and palette methods, with deterministic synthetic world/DAT rows."""
import argparse,hashlib,subprocess,tempfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args();own=Path(__file__).parent;base=a.source/'Source'
def method(s,sig):
 start=s.index(sig);op=s.index('{',start);d=0
 for i in range(op,len(s)):
  d+=(s[i]=='{')-(s[i]=='}')
  if d==0:return s[start:i+1]
s=(base/'ACE.Server/Factories/LootGenerationFactory.cs').read_text(encoding='utf-8-sig');methods='\n'.join(method(s,n) for n in ['private static MaterialType GetMaterialType(','private static MaterialType GetDefaultMaterialType(','private static void MutateColor('])
h=(own/'harness.cs').read_text().replace('// METHODS',methods)
files=[base/'ACE.Entity/Enum'/f'{n}.cs' for n in ['MaterialType','WeenieType','ItemType']]+[base/'ACE.Server/Factories/LootTables.cs']
# Only DefaultMaterial initializer is used, retaining it verbatim.
loot=files.pop().read_text(encoding='utf-8-sig');st=loot.index('public static int[][] DefaultMaterial');op=loot.index('{',st);end=loot.index('};',op)+2
h=h.replace('// DEFAULT','public static class LootTables{'+loot[st:end]+'}')
with tempfile.TemporaryDirectory(prefix='bace-material-oracle-') as directory:
 tmp=Path(directory)
 for i,f in enumerate(files):(tmp/f'Source{i}.cs').write_bytes(f.read_bytes())
 (tmp/'Harness.cs').write_text(h);(tmp/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><RollForward>Major</RollForward><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 subprocess.run([a.dotnet,'build','--nologo','-o',str(tmp/'out'),str(tmp/'Oracle.csproj')],check=True)
 out=subprocess.check_output([a.dotnet,str(tmp/'out/Oracle.dll')],text=True)
(own/'../../tests/fixtures/materials.csv').write_text('# Official ACEmulator/ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b; AGPL-3.0-only\n# sha256 '+hashlib.sha256(s.encode()).hexdigest()+' LootGenerationFactory.cs\n'+out)
