#!/usr/bin/env python3
"""Compile original Monster_Inventory methods; synthetic ownership/RNG adapters."""
import argparse,hashlib,subprocess,tempfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args();own=Path(__file__).parent;base=a.source/'Source'
def method(s,sig):
 start=s.index(sig);op=s.index('{',start);d=0
 for i in range(op,len(s)):
  d+=(s[i]=='{')-(s[i]=='}')
  if d==0:return s[start:i+1]
s=(base/'ACE.Server/WorldObjects/Monster_Inventory.cs').read_text(encoding='utf-8-sig')
names=['public List<WorldObject> SelectWieldedTreasure(','public List<WorldObject> SelectWieldedClothing(','public List<WorldObject> SelectWieldedArmor(','public int ValidLocationComparer(','public int ArmorLevelComparer(','public void GetMonsterInventory(','public List<WorldObject> SelectWieldedWeapons(','public WorldObject FindInventoryWeapon(','public WorldObject SelectWieldedShield(','public void EquipInventoryItems(']
equip=(base/'ACE.Server/WorldObjects/Creature_Equipment.cs').read_text(encoding='utf-8-sig')
death=(base/'ACE.Server/WorldObjects/Creature_Death.cs').read_text(encoding='utf-8-sig')
start=death.index('var dropFlags =');stop=death.index('if (TryDequipObjectWithBroadcasting',start)
drop=death[start:stop]+'results.Add(item); } return results;'
h=(own/'harness.cs').read_text().replace('// METHODS','\n'.join(method(s,n) for n in names)).replace('// DROP',drop).replace('// PLACEMENT',method(equip,'private static void GetPlacementLocation(')).replace('// SLOTS',method(equip,'private static bool IsWeaponSlot('))
files=[base/'ACE.Entity/Enum'/f'{n}.cs' for n in ['WeenieType','CoverageMask','EquipMask','CombatStyle','ItemType','Placement','ParentLocation','DestinationType','BondedStatus']]+[base/'ACE.Common/Extensions/ListExtensions.cs']
with tempfile.TemporaryDirectory(prefix='bace-equipment-oracle-') as directory:
 tmp=Path(directory)
 for i,f in enumerate(files):(tmp/f'Source{i}.cs').write_bytes(f.read_bytes())
 (tmp/'Harness.cs').write_text(h);(tmp/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><RollForward>Major</RollForward><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 subprocess.run([a.dotnet,'build','--nologo','-o',str(tmp/'out'),str(tmp/'Oracle.csproj')],check=True)
 out=subprocess.check_output([a.dotnet,str(tmp/'out/Oracle.dll')],text=True)
 drop_out='\n'.join(l[5:] for l in out.splitlines() if l.startswith('DROP|'))+'\n'
 out='\n'.join(l for l in out.splitlines() if not l.startswith('DROP|'))+'\n'
(own/'../../tests/fixtures/equipment.csv').write_text('# Official ACEmulator/ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b; AGPL-3.0-only\n# sha256 '+hashlib.sha256(s.encode()).hexdigest()+' Monster_Inventory.cs\n'+out)

(own/'../../tests/fixtures/equipment_drop.csv').write_text('# Official ACE Creature_Death.cs source selection; AGPL-3.0-only\n# sha256 '+hashlib.sha256(death.encode()).hexdigest()+'\n'+drop_out)
