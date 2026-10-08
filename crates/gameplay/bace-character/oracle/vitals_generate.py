"""Compile pinned original vital/attribute/formula arithmetic, never a Rust mirror."""
import argparse, hashlib, subprocess, tempfile, urllib.request
from pathlib import Path
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args()
rels=['Source/ACE.Server/WorldObjects/Entity/CreatureVital.cs','Source/ACE.Server/WorldObjects/Entity/CreatureAttribute.cs','Source/ACE.Server/Entity/AttributeFormula.cs','Source/ACE.Common/Extensions/FloatExtensions.cs']
sources=[];provenance=[]
for rel in rels:
 data=(a.source/rel).read_bytes();assert data==urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{rel}',timeout=30).read();sources.append(data.decode('utf-8-sig'));provenance.append(f'# sha256 {hashlib.sha256(data).hexdigest()} {rel}')
def method(source,sig):
 start=source.index(sig);op=source.index('{',start);depth=0
 for i in range(op,len(source)):
  depth+=(source[i]=='{')-(source[i]=='}')
  if depth==0:return source[start:i+1]
 raise ValueError(sig)
h=Path(__file__).with_name('vitals_harness.cs').read_text()
for marker,source,sig in [('VITAL',sources[0],'public uint GetMaxValue(bool enchanted)'),('ATTRIBUTE',sources[1],'public uint GetCurrent(bool enchanted)'),('FORMULA',sources[2],'public static uint GetFormula(Creature creature, DatLoader.Entity.SkillFormula formula, bool current = true)')]:h=h.replace('// '+marker,method(source,sig))
with tempfile.TemporaryDirectory(prefix='bace-vitals-') as td:
 b=Path(td);(b/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>');(b/'Program.cs').write_text(h);(b/'FloatExtensions.cs').write_text(sources[3]);subprocess.run([a.dotnet,'build','--nologo','-o',str(b/'out'),str(b/'Oracle.csproj')],check=True);out=subprocess.check_output([a.dotnet,str(b/'out/Oracle.dll')],text=True)
Path(__file__).parent.parent.joinpath('tests/fixtures/vitals.csv').write_text(f'# official ACE {PIN}\n'+'\n'.join(provenance)+'\n'+out)
