#!/usr/bin/env python3
"""Original armor routing/resistance/gear methods with synthetic item properties."""
import argparse,subprocess,tempfile,hashlib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args();base=a.source/'Source';own=Path(__file__).parent
s=(base/'ACE.Server/Factories/LootGenerationFactory_Clothing.cs').read_text(encoding='utf-8-sig')
def method(sig):
 start=s.index(sig);op=s.index('{',start);d=0
 for i in range(op,len(s)):
  d+=(s[i]=='{')-(s[i]=='}')
  if d==0:return s[start:i+1]
methods='\n'.join(method(n) for n in ['private static string GetMutationScript_ArmorLevel(','private static bool TryMutateArmorModVsType(','private static bool TryMutateGearRating(','private static void SetWieldLevelReq('])
h=(own/'harness.cs').read_text().replace('// METHODS',methods)
files=[base/f'ACE.Server/Factories/Tables/{n}.cs' for n in ['ArmorModVsTypeChance','GearRatingChance','EquipmentSetChance']]+[base/f'ACE.Entity/Enum/{n}.cs' for n in ['CoverageMask','WieldRequirement','Skill','EquipmentSet']]+[base/f'ACE.Server/Factories/Enum/{n}.cs' for n in ['TreasureItemType','TreasureArmorType']]+[base/'ACE.Server/Factories/Entity/ChanceTable.cs']
with tempfile.TemporaryDirectory(prefix='bace-armor-oracle-') as directory:
 tmp=Path(directory)
 for i,f in enumerate(files):(tmp/f'Source{i}.cs').write_bytes(f.read_bytes())
 (tmp/'Harness.cs').write_text(h);(tmp/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><RollForward>Major</RollForward><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 subprocess.run([a.dotnet,'build','--nologo','-o',str(tmp/'out'),str(tmp/'Oracle.csproj')],check=True)
 out=subprocess.check_output([a.dotnet,str(tmp/'out/Oracle.dll')],text=True)
(own/'../../tests/fixtures/armor.csv').write_text('# Official ACEmulator/ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b; AGPL-3.0-only\n# sha256 '+hashlib.sha256(s.encode()).hexdigest()+' LootGenerationFactory_Clothing.cs\n'+out)
