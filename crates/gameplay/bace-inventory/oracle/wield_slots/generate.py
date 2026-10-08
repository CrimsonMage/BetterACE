#!/usr/bin/env python3
"""Compile unchanged pinned ACE collision and coverage/parent-location methods."""
from pathlib import Path
import subprocess,tempfile,os,hashlib
ROOT=Path(__file__).resolve().parents[5]
SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
a=(SRC/'ACE.Server/WorldObjects/Player_Inventory.cs').read_text();b=(SRC/'ACE.Server/WorldObjects/Creature_Equipment.cs').read_text()
def method(text,name):
 start=text.rfind('\n',0,text.index(name))+1;begin=text.index('{',start);depth=1;end=begin+1
 while depth:
  if text[end]=='{':depth+=1
  elif text[end]=='}':depth-=1
  end+=1
 return text[start:end]
methods=method(a,'private bool CheckWeaponCollision(')+'\n'+ '\n'.join(method(b,n) for n in ['public List<WorldObject> GetEquippedItems(', 'public List<WorldObject> GetEquippedClothingArmor(', 'private static void GetPlacementLocation(', 'private static bool IsWeaponSlot('])
code='''using System;using System.Linq;using System.Collections.Generic;using ACE.Entity.Enum;
class Log { public void Warn(string s){} }
class WorldObject { public string Name="item",Guid="id";public EquipMask? CurrentWieldedLocation;public ParentLocation? ParentLocation;public CoverageMask? ClothingPriority;public ItemType ItemType;public CombatStyle? DefaultCombatStyle;public Skill WeaponSkill;public int? AmmoType;public bool IsTwoHanded=>WeaponSkill==Skill.TwoHandedCombat;public bool IsCaster=>DefaultCombatStyle==CombatStyle.Magic;public bool IsAmmoLauncher=>DefaultCombatStyle==CombatStyle.Bow||DefaultCombatStyle==CombatStyle.Crossbow||DefaultCombatStyle==CombatStyle.Atlatl; }
class Clothing:WorldObject{}
class Player:WorldObject {
public Log log=new();public CombatMode CombatMode=CombatMode.NonCombat;public Dictionary<int,WorldObject> EquippedObjects=new();
public WorldObject GetEquippedMainHand()=>EquippedObjects.Values.FirstOrDefault(i=>i.ParentLocation==ACE.Entity.Enum.ParentLocation.RightHand&&(i.CurrentWieldedLocation==EquipMask.MeleeWeapon||i.CurrentWieldedLocation==EquipMask.TwoHanded))??EquippedObjects.Values.FirstOrDefault(i=>i.CurrentWieldedLocation==EquipMask.MissileWeapon)??EquippedObjects.Values.FirstOrDefault(i=>i.CurrentWieldedLocation==EquipMask.Held);
public WorldObject GetEquippedOffHand()=>EquippedObjects.Values.FirstOrDefault(i=>i.CurrentWieldedLocation==EquipMask.Shield);
public WorldObject GetEquippedAmmo()=>EquippedObjects.Values.FirstOrDefault(i=>i.CurrentWieldedLocation==EquipMask.MissileAmmo);
METHODS
public bool Check(WorldObject item,uint loc)=>CheckWeaponCollision(item,(EquipMask)loc);
public int Overlap(WorldObject item,uint loc)=>GetEquippedItems(item,(EquipMask)loc).Count;
public void Add(int n,int kind,uint loc){var i=Make(kind);i.CurrentWieldedLocation=(EquipMask)loc;GetPlacementLocation(i,(EquipMask)loc,out var p,out var parent);if(parent!=0)i.ParentLocation=parent;EquippedObjects[n]=i;}
public static WorldObject Make(int kind){WorldObject w=kind==8||kind==9?new Clothing():new WorldObject();w.ItemType=ItemType.MeleeWeapon;w.DefaultCombatStyle=CombatStyle.OneHanded;switch(kind){case 1:w.WeaponSkill=Skill.TwoHandedCombat;break;case 2:w.DefaultCombatStyle=CombatStyle.Magic;break;case 3:w.DefaultCombatStyle=CombatStyle.Bow;w.AmmoType=1;break;case 4:w.DefaultCombatStyle=CombatStyle.Atlatl;w.AmmoType=2;break;case 5:w.DefaultCombatStyle=CombatStyle.ThrownWeapon;w.AmmoType=3;break;case 6:w.ItemType=ItemType.Armor;break;case 7:w.DefaultCombatStyle=null;break;case 8:w.ClothingPriority=(CoverageMask)3;w.ItemType=ItemType.Clothing;break;case 9:w.ClothingPriority=(CoverageMask)4;w.ItemType=ItemType.Armor;break;case 10:w.AmmoType=1;break;}return w;}
}
class Program{static void Main(){uint[] locs={0x100000,0x200000,0x400000,0x800000,0x1000000,0x2000000,0x8000,3};foreach(int kind in Enumerable.Range(0,11))foreach(int gear in Enumerable.Range(0,12))foreach(uint loc in locs)foreach(int mode in new[]{1,2}){var p=new Player{CombatMode=(CombatMode)mode};switch(gear){case 1:p.Add(0,0,0x100000);break;case 2:p.Add(0,1,0x2000000);break;case 3:p.Add(0,2,0x1000000);break;case 4:p.Add(0,3,0x400000);break;case 5:p.Add(0,6,0x200000);break;case 6:p.Add(0,0,0x200000);break;case 7:p.Add(0,10,0x800000);break;case 8:p.Add(0,4,0x800000);break;case 9:p.Add(0,8,3);break;case 10:p.Add(0,9,3);break;case 11:p.Add(0,0,0x100000);p.Add(1,6,0x200000);break;}var w=Player.Make(kind);Console.WriteLine($"{kind}|{gear}|{loc}|{mode}|{(p.Check(w,loc)?1:0)}|{p.Overlap(w,loc)}");}}}
'''.replace('METHODS',methods)
enums=['EquipMask','ParentLocation','Placement','CoverageMask','ItemType','CombatStyle','Skill','CombatMode']
# Skill contains extensions requiring unrelated source types; retain only enum.
extra=''
for name in enums:
 text=(SRC/f'ACE.Entity/Enum/{name}.cs').read_text()
 if name=='Skill':
  start=text.index('public enum Skill');brace=text.index('{',start);end=text.index('\n    }',brace)+6;text='namespace ACE.Entity.Enum {'+text[start:end]+'}'
 extra+='\n'+text.replace('using System;','')
code+=extra
with tempfile.TemporaryDirectory(prefix='betterace-slots-') as d:
 p=Path(d);(p/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>');(p/'Program.cs').write_text(code)
 try:out=subprocess.check_output([os.environ.get('DOTNET','/tmp/bace-crafting-dotnet/dotnet'),'run','--project',str(p/'Oracle.csproj'),'-c','Release'],text=True,stderr=subprocess.STDOUT)
 except subprocess.CalledProcessError as e:print(e.output);raise
 rows='\n'.join(l for l in out.splitlines() if l and l[0].isdigit() and '|' in l)+'\n'
 (ROOT/'crates/gameplay/bace-inventory/tests/fixtures/wield_slots.txt').write_text('# ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b original methods\n'+rows);print(len(rows.splitlines()),'source vectors')
