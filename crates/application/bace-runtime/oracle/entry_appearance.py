#!/usr/bin/env python3
"""Compile unchanged pinned ACE appearance methods against synthetic DAT inputs."""
from pathlib import Path
import subprocess,tempfile,hashlib
ROOT=Path(__file__).resolve().parents[4];SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
def method(path,signature):
 s=(SRC/path).read_text();a=s.index(signature);start=s.index('{',a);depth=1;i=start+1
 while depth:
  depth+=(s[i]=='{')-(s[i]=='}');i+=1
 return s[a:i]
files={'BASE_METHOD':('ACE.Server/WorldObjects/WorldObject_Networking.cs','protected void AddBaseModelData('),'WORLD_METHOD':('ACE.Server/WorldObjects/WorldObject_Networking.cs','public virtual ACE.Entity.ObjDesc CalculateObjDesc('),'CREATURE_METHOD':('ACE.Server/WorldObjects/Creature_Networking.cs','public override ACE.Entity.ObjDesc CalculateObjDesc('),'SETUP_METHOD':('ACE.Server/WorldObjects/Creature_Networking.cs','protected ACE.Entity.ObjDesc AddSetupAsClothingBase('),'PRIORITY_METHOD':('ACE.DatLoader/FileTypes/ClothingTable.cs','public CoverageMask? GetVisualPriority('),'PALETTE_METHOD':('ACE.DatLoader/FileTypes/PaletteSet.cs','public uint GetPaletteID(')}
s=Path(__file__).with_name('entry').joinpath('appearance.cs').read_text()
for token,(p,sig) in files.items():s=s.replace(token,method(p,sig))
with tempfile.TemporaryDirectory(prefix='entry-appearance-') as tmp:
 p=Path(tmp);(p/'Program.cs').write_text(s)
 names=['SetupConst','CoverageMask','EquipMask','ItemType','CharacterOption','CharacterOptions1','CharacterOptions2','CharacterOptions1Attribute','CharacterOptions2Attribute','HeritageGroup']
 inputs=list(dict.fromkeys(v[0] for v in files.values()))+['ACE.Entity/ObjDesc.cs']+['ACE.Entity/'+('' if n.endswith('Attribute') else 'Enum/')+n+'.cs' for n in names]
 for name in names:(p/(name+'.cs')).write_bytes((SRC/('ACE.Entity/'+('' if name.endswith('Attribute') else 'Enum/')+name+'.cs')).read_bytes())
 (p/'ObjDesc.cs').write_bytes((SRC/'ACE.Entity/ObjDesc.cs').read_bytes())
 (p/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
 subprocess.run(['/tmp/bace-crafting-dotnet/dotnet','build','--nologo','-o',str(p/'build')],cwd=p,check=True)
 out=subprocess.check_output(['/tmp/bace-crafting-dotnet/dotnet',str(p/'build/oracle.dll')],text=True)
 Path(__file__).parents[1].joinpath('tests/fixtures/entry_appearance.txt').write_text(''.join('# '+v+' '+hashlib.sha256((SRC/v).read_bytes()).hexdigest()+'\n' for v in inputs)+out)
