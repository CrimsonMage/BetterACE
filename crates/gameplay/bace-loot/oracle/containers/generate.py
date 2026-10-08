#!/usr/bin/env python3
"""Original pinned Container.GenerateContainList + both TryAddToInventory methods.
Compiles unchanged method bodies with factory/property boundaries only stubbed.
"""
from pathlib import Path
import json,subprocess,sys,tempfile,hashlib,re
ROOT=Path(__file__).resolve().parents[5]
SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
source=(SRC/'ACE.Server/WorldObjects/Container.cs').read_text()
def method(sig):
 start=source.index(sig);i=source.index('{',start);depth=1;i+=1
 while depth:depth+=(source[i]=='{')-(source[i]=='}');i+=1
 return source[start:i]
def item(id,type=1,main=0,pack=0,side=False,rows=[]):return dict(id=id,type=type,main=main,pack=pack,side=side,rows=rows)
def row(id,dest=1,shade=0,stack=1,palette=0):return dict(id=id,dest=dest,shade=shade,stack=stack,palette=palette)
cases=[]
for main in [0,1,2,4]:
 for pack in [0,1,2]:
  cases.append([item(1,21,main,pack,rows=[row(2,shade=.5),row(3,9,shade=.25),row(4,shade=-1),row(5),row(6,2)]),item(2,21,1,0,True,[row(7,9,shade=.3)]),item(3),item(4),item(5,21,1,0,True),item(6),item(7)])
for shade in [-1,0,.125,1]:
 for dest in [1,9,2,3]:cases.append([item(1,21,5,5,rows=[row(2,dest,shade,3,4)]),item(2,51)])
for count in [2,16,17,20,33]:
 cases.append([item(1,21,0,count,rows=[row(i) for i in range(2,count+2)]+[row(100)])]+[item(i,21,1,0,True) for i in range(2,count+2)]+[item(100)])
cases.append([item(1,1,5,5,rows=[row(2)]),item(2)])
cases.append([item(1,21,5,5,rows=[row(999),row(2)]),item(2)])
for type in [14,20,21,55,56,57,16]:
 cases.append([item(1,type,5,5,rows=[row(2)]),item(2)])
# A generic object with capacity properties is not a Container for fallback.
cases.append([item(1,21,0,1,rows=[row(2),row(3)]),item(2,1,5,0,True),item(3)])
# The complete Patches WCID 30997 authors both capacities as -1. Execute the
# unchanged Container.TryAddToInventory body for direct rejection and sidepack
# fallback, rather than deriving that signed behavior from the Rust branch.
cases.append([item(1,21,-1,-1,rows=[row(2)]),item(2)])
cases.append([item(1,21,-1,1,rows=[row(2),row(3)]),item(2,21,1,0,True),item(3)])
# Derive constructor inheritance from the pinned factory/classes, not our Rust
# classification. Only non-Creature Containers call GenerateContainList here.
factory=(SRC/'ACE.Server/Factories/WorldObjectFactory.cs').read_text()
enums=(SRC/'ACE.Entity/Enum/WeenieType.cs').read_text()
names=re.findall(r'^        (\w+)\s*[, ]',enums,re.M)
values={name:n for n,name in enumerate(names)}
parents={}
for path in (SRC/'ACE.Server/WorldObjects').glob('*.cs'):
 for name,parent in re.findall(r'class (\w+)\s*:\s*(\w+)',path.read_text()):parents[name]=parent
def is_subclass(name,target):
 for _ in range(20):
  if name==target:return True
  name=parents.get(name)
  if name is None:return False
 raise ValueError('class inheritance cycle')
container_types=set()
for name,cls in re.findall(r'case WeenieType\.(\w+):\s*return new (\w+)',factory):
 if is_subclass(cls,'Container') and not is_subclass(cls,'Creature'):container_types.add(values[name])
with tempfile.TemporaryDirectory(prefix='bace-container-oracle-') as tmp:
 out=Path(tmp);(out/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 code=(Path(__file__).parent/'harness.cs').read_text().replace('/*CONTAINER_TYPES*/',' or '.join(map(str,sorted(container_types))))
 for tag,sig in [('CONTAIN','public void GenerateContainList()'),('ADD1','public bool TryAddToInventory(WorldObject worldObject, int placementPosition'),('ADD2','public bool TryAddToInventory(WorldObject worldObject, out Container container')]:code=code.replace('/*'+tag+'*/',method(sig))
 (out/'Program.cs').write_text(code);(out/'inputs.json').write_text(json.dumps(cases))
 dotnet=sys.argv[1] if len(sys.argv)>1 else '/tmp/bace-crafting-dotnet/dotnet'
 result=subprocess.run([dotnet,'build','--nologo','-o',str(out/'build')],cwd=out,capture_output=True,text=True)
 if result.returncode:print(result.stdout);result.check_returncode()
 run=subprocess.run([dotnet,str(out/'build/oracle.dll')],cwd=out,capture_output=True,text=True,check=True)
 data=json.loads(run.stdout);data['factory_sha256']=hashlib.sha256(factory.encode()).hexdigest();data['source_sha256']=hashlib.sha256((SRC/'ACE.Server/WorldObjects/Container.cs').read_bytes()).hexdigest()
 path=ROOT/'crates/gameplay/bace-loot/tests/fixtures/container_contents.json';path.write_text(json.dumps(data,indent=2)+'\n');print(path)
