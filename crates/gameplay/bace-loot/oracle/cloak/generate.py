#!/usr/bin/env python3
"""Compose original cloak, value, material fallback and gear-rating methods."""
import argparse,subprocess,tempfile,hashlib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args();own=Path(__file__).parent;base=a.source/'Source'
def method(s,sig):
 start=s.index(sig);op=s.index('{',start);d=0
 for i in range(op,len(s)):
  d+=(s[i]=='{')-(s[i]=='}')
  if not d:return s[start:i+1]
sources={n:(base/'ACE.Server/Factories'/n).read_text(encoding='utf-8-sig') for n in ['LootGenerationFactory.cs','LootGenerationFactory_Clothing.cs','LootGenerationFactory_Gem.cs']}
methods=[]
for name,names in {'LootGenerationFactory.cs':['private static MaterialType GetDefaultMaterialType(','private static void MutateValue(','private static void MutateValue_Generic(','private static void MutateValue_Spells('],'LootGenerationFactory_Clothing.cs':['private static void MutateCloak(','private static bool TryMutateGearRating(','private static void SetWieldLevelReq(','private static void MutateValue_Armor('],'LootGenerationFactory_Gem.cs':['private static void MutateValue_Gem(']}.items():
 for n in names:methods.append(method(sources[name],n))
h=(own/'harness.cs').read_text().replace('// METHODS','\n'.join(methods))
loot=(base/'ACE.Server/Factories/LootTables.cs').read_text(encoding='utf-8-sig');st=loot.index('public static int[][] DefaultMaterial');op=loot.index('{',st);end=loot.index('};',op)+2;h=h.replace('// DEFAULT','public static class LootTables{'+loot[st:end]+'}')
files=[base/f'ACE.Server/Factories/Tables/{n}.cs' for n in ['CloakChance','WorkmanshipChance','GearRatingChance','MaterialTable','GemMaterialChance']]+[base/f'ACE.Entity/Enum/{n}.cs' for n in ['MaterialType','WeenieType','ItemType','EquipmentSet','SpellId','Skill','WieldRequirement']]+[base/f'ACE.Server/Factories/Enum/{n}.cs' for n in ['WeenieClassName','TreasureArmorType','TreasureItemType']]+[base/'ACE.Server/Factories/Entity/ChanceTable.cs',base/'ACE.Server/Factories/Entity/GemResult.cs']
with tempfile.TemporaryDirectory(prefix='bace-cloak-oracle-') as directory:
 tmp=Path(directory)
 for i,f in enumerate(files):(tmp/f'Source{i}.cs').write_bytes(f.read_bytes())
 (tmp/'Harness.cs').write_text(h);(tmp/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><RollForward>Major</RollForward><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 subprocess.run([a.dotnet,'build','--nologo','-o',str(tmp/'out'),str(tmp/'Oracle.csproj')],check=True);out=subprocess.check_output([a.dotnet,str(tmp/'out/Oracle.dll')],text=True)
(own/'../../tests/fixtures/cloak.csv').write_text('# Official ACEmulator/ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b; AGPL-3.0-only\n'+''.join('# sha256 '+hashlib.sha256(s.encode()).hexdigest()+' '+n+'\n' for n,s in sources.items())+out)
