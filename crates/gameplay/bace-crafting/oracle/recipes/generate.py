#!/usr/bin/env python3
"""Execute unchanged pinned MutationCache parser and Effect engine on synthetic qualities.
All recipe scripts are original embedded resources; no Rust formula computes expected values.
"""
from pathlib import Path
import subprocess,tempfile,hashlib,re,sys
ROOT=Path(__file__).resolve().parents[5]
SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
MUT=SRC/'ACE.Server/Entity/Mutations'
def enum(path,name):
 text=path.read_text(encoding='utf-8-sig');start=text.index('public enum '+name);left=text.index('{',start);i=left+1;depth=1
 while depth:depth+=(text[i]=='{')-(text[i]=='}');i+=1
 return re.sub(r'\[[^\]\n]*\]','',text[start:i])
with tempfile.TemporaryDirectory(prefix='bace-recipe-script-oracle-') as tmp:
 out=Path(tmp); hashes=[]
 for p in MUT.glob('*.cs'):
  data=p.read_bytes();(out/p.name).write_bytes(data);hashes.append((str(p.relative_to(SRC)),hashlib.sha256(data).hexdigest()))
 resources=[]
 for p in sorted((MUT/'Recipes').glob('*.txt')):
  (out/p.name).write_bytes(p.read_bytes());resources.append(f'<EmbeddedResource Include="{p.name}" LogicalName="ACE.Server.Entity.Mutations.Recipes.{p.name}" />');hashes.append((str(p.relative_to(SRC)),hashlib.sha256(p.read_bytes()).hexdigest()))
 (out/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup><ItemGroup>'+''.join(resources)+'</ItemGroup></Project>')
 code='namespace ACE.Entity.Enum {'
 for name in ['Skill','ImbuedEffectType','WieldRequirement','EffectArgumentType','MutationEffectType','StatType']:
  p=SRC/f'ACE.Entity/Enum/{name}.cs';code+=enum(p,name);hashes.append((str(p.relative_to(SRC)),hashlib.sha256(p.read_bytes()).hexdigest()))
 code+='}\nnamespace ACE.Entity.Enum.Properties {'
 for name in ['PropertyInt','PropertyInt64','PropertyBool','PropertyFloat','PropertyDataId']:
  p=SRC/f'ACE.Entity/Enum/Properties/{name}.cs';code+=enum(p,name);hashes.append((str(p.relative_to(SRC)),hashlib.sha256(p.read_bytes()).hexdigest()))
 code+='}'
 (out/'Enums.cs').write_text(code)
 (out/'Program.cs').write_text((Path(__file__).parent/'harness.cs').read_text())
 dotnet=sys.argv[1] if len(sys.argv)>1 else '/tmp/bace-crafting-dotnet/dotnet'
 result=subprocess.run([dotnet,'build','--nologo','-o',str(out/'build')],cwd=out,capture_output=True,text=True)
 if result.returncode:print(result.stdout);result.check_returncode()
 run=subprocess.run([dotnet,str(out/'build/oracle.dll')],cwd=out,capture_output=True,text=True,check=True)
 dest=ROOT/'crates/gameplay/bace-crafting/tests/fixtures/recipe_scripts.tsv'
 dest.write_text('# ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b original parser + Effect engine\n'+''.join(f'# sha256 {h} {p}\n' for p,h in hashes)+run.stdout)
 print(dest)
