#!/usr/bin/env python3
"""Compile official ACE WCID routing/table code; no Rust or generated tables imported."""
import argparse, hashlib, subprocess, tempfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args()
base=a.source/'Source'; own=Path(__file__).parent
files=list((base/'ACE.Server/Factories/Tables/Wcids').rglob('*.cs'))
for name in ['TreasureProfile_Item','TreasureProfile_MagicItem','TreasureProfile_Mundane','GemClassChance','GemMaterialChance','WeaponTypeChance','ArmorTypeChance','HeritageChance','PetDeviceChance']:
 files.append(base/f'ACE.Server/Factories/Tables/{name}.cs')
files += [base/'ACE.Server/Factories/Entity/ChanceTable.cs',base/'ACE.Server/Factories/Entity/GemResult.cs']
for name in ['WeenieClassName','TreasureItemType','TreasureItemCategory','TreasureArmorType','TreasureWeaponType','TreasureHeritageGroup','MeleeWeaponSkill','SocietyArmorType','SocietyType','Level8_SpellComponentType']:
 files.append(base/f'ACE.Server/Factories/Enum/{name}.cs')
for name in ['MaterialType','Skill','HeritageGroup']:
 files.append(base/f'ACE.Entity/Enum/{name}.cs')
def method(s,sig):
 start=s.index(sig);op=s.index('{',start);d=0
 for i in range(op,len(s)):
  d+=(s[i]=='{')-(s[i]=='}')
  if not d:return s[start:i+1]
factory=base/'ACE.Server/Factories/LootGenerationFactory.cs';source=factory.read_text(encoding='utf-8-sig')
h=(own/'harness.cs').read_text().replace('// ROUTING',method(source,'private static TreasureRoll RollWcid(')).replace('// CATEGORY',method(source,'private static TreasureItemType RollItemType('))
provenance='# Official ACEmulator/ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b; AGPL-3.0-only\n'
with tempfile.TemporaryDirectory(prefix='bace-selection-oracle-') as temp:
 root=Path(temp)
 for i,f in enumerate(files):
  data=f.read_bytes();(root/f'Source{i}.cs').write_bytes(data);provenance+=f'# sha256 {hashlib.sha256(data).hexdigest()} {f.relative_to(a.source)}\n'
 (root/'Harness.cs').write_text(h)
 (root/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><RollForward>Major</RollForward></PropertyGroup></Project>')
 subprocess.run([a.dotnet,'build','--nologo','-o',str(root/'out'),str(root/'Oracle.csproj')],check=True)
 output=subprocess.check_output([a.dotnet,str(root/'out/Oracle.dll')],text=True)
(own/'../../tests/fixtures/selection.csv').write_text(provenance+output)
