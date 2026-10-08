#!/usr/bin/env python3
"""Execute original monetary, burden and quality methods with synthetic item properties."""
import argparse,hashlib,subprocess,tempfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args();root=a.source/'Source';own=Path(__file__).parent

def method(text,signature):
 start=text.index(signature);op=text.index('{',start);depth=0
 for i in range(op,len(text)):
  depth+=(text[i]=='{')-(text[i]=='}')
  if not depth:return text[start:i+1]

sources={}
for name in ['LootGenerationFactory.cs','LootGenerationFactory_Clothing.cs','LootGenerationFactory_Gem.cs']:
 f=root/'ACE.Server/Factories'/name;sources[name]=f.read_text(encoding='utf-8-sig')
methods=[]
for name,signatures in {
 'LootGenerationFactory.cs':['private static bool MutateBurden(','private static void MutateValue(','private static void MutateValue_Generic(','private static void MutateValue_Spells('],
 'LootGenerationFactory_Clothing.cs':['private static void MutateValue_Armor('],
 'LootGenerationFactory_Gem.cs':['private static void MutateValue_Gem('],
}.items():
 for signature in signatures:methods.append(method(sources[name],signature))
h=(own/'harness.cs').read_text().replace('// METHODS','\n'.join(methods))
files=[]
for name in ['MaterialTable','WorkmanshipChance','QualityChance','GemMaterialChance']:
 files.append(root/f'ACE.Server/Factories/Tables/{name}.cs')
files += [root/'ACE.Server/Factories/Entity/GemResult.cs',root/'ACE.Server/Factories/Entity/ChanceTable.cs',root/'ACE.Server/Factories/Enum/WeenieClassName.cs',root/'ACE.Entity/Enum/MaterialType.cs']
provenance='# Official ACEmulator/ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b; AGPL-3.0-only\n'
with tempfile.TemporaryDirectory(prefix='bace-value-oracle-') as directory:
 tmp=Path(directory)
 for i,f in enumerate(files):
  data=f.read_bytes();(tmp/f'Source{i}.cs').write_bytes(data);provenance+=f'# sha256 {hashlib.sha256(data).hexdigest()} {f.relative_to(a.source)}\n'
 for name,source in sources.items():provenance+=f'# sha256 {hashlib.sha256(source.encode()).hexdigest()} {name}\n'
 (tmp/'Harness.cs').write_text(h);(tmp/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><RollForward>Major</RollForward><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 subprocess.run([a.dotnet,'build','--nologo','-o',str(tmp/'out'),str(tmp/'Oracle.csproj')],check=True)
 out=subprocess.check_output([a.dotnet,str(tmp/'out/Oracle.dll')],text=True)
(own/'../../tests/fixtures/value.csv').write_text(provenance+out)
