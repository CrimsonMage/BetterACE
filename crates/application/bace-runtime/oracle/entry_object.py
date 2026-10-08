#!/usr/bin/env python3
"""Original ACE optional-field/description/physics flag methods, synthetic inputs."""
from pathlib import Path
import subprocess,tempfile,hashlib
ROOT=Path(__file__).resolve().parents[4];SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
path=SRC/'ACE.Server/WorldObjects/WorldObject_Networking.cs';source=path.read_text()
def method(signature):
 a=source.index(signature);i=source.index('{',a)+1;depth=1
 while depth:depth+=(source[i]=='{')-(source[i]=='}');i+=1
 return source[a:i]
s=Path(__file__).with_name('entry').joinpath('object.cs').read_text()
for token,sig in [('FLAGS1','protected WeenieHeaderFlag CalculateWeenieHeaderFlag('),('FLAGS2','private WeenieHeaderFlag2 CalculateWeenieHeaderFlag2('),('DESCRIPTION','private void UpdateObjectDescriptionFlags('),('UPDATE','private void UpdateObjectDescriptionFlag('),('PHYSICS','protected PhysicsDescriptionFlag CalculatedPhysicsDescriptionFlag(')]:s=s.replace(token,method(sig))
files=['WeenieHeaderFlags','ObjectDescriptionFlag','PhysicsDescriptionFlag','WeenieType','CloakStatus','PlayerKillerStatus','HouseType']
with tempfile.TemporaryDirectory(prefix='entry-object-') as tmp:
 p=Path(tmp);(p/'Program.cs').write_text(s);inputs=[path]
 for name in files:
  f=SRC/('ACE.Entity/Enum/'+name+'.cs');inputs.append(f);(p/(name+'.cs')).write_bytes(f.read_bytes())
 (p/'Properties.cs').write_text('namespace ACE.Entity.Enum.Properties{enum PropertyDataId{PhysicsScript}enum PropertyInt{HookItemType}}')
 (p/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
 subprocess.run(['/tmp/bace-crafting-dotnet/dotnet','build','--nologo','-o',str(p/'build')],cwd=p,check=True)
 out=subprocess.check_output(['/tmp/bace-crafting-dotnet/dotnet',str(p/'build/oracle.dll')],text=True)
 Path(__file__).parents[1].joinpath('tests/fixtures/entry_object.csv').write_text(''.join('# '+str(v.relative_to(SRC))+' '+hashlib.sha256(v.read_bytes()).hexdigest()+'\n' for v in inputs)+out)
