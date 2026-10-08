#!/usr/bin/env python3
"""Unmodified pinned ACE wield requirement methods; only surrounding getters stubbed."""
from pathlib import Path
import subprocess,tempfile,os,hashlib
ROOT=Path(__file__).resolve().parents[5]
ACE=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
a=(ACE/'ACE.Server/WorldObjects/Player_Inventory.cs').read_text()
def method(name):
 start=a.rfind('\n',0,a.index(name))+1;begin=a.index('{',start);depth=1;end=begin+1
 while depth:
  if a[end]=='{':depth+=1
  elif a[end]=='}':depth-=1
  end+=1
 return a[start:end]
methods=method('private WeenieError CheckWieldRequirements(')+'\n'+method('private WeenieError CheckWieldRequirement(')
code='''using System;using System.Collections.Generic;using ACE.Entity.Enum;
namespace ACE.Entity.Enum {enum CreatureType{Invalid=0,Human=31}}
enum WieldRequirement{Invalid,Skill,RawSkill,Attrib,RawAttrib,SecondaryAttrib,RawSecondaryAttrib,Level,Training,IntStat,BoolStat,CreatureType,HeritageType}
enum WeenieError{None=0,SkillTooLow=0x468,LevelTooLow=0x420,YouDoNotOwnThatItem=0x4be,HeritageRequiresSpecificArmor=0x585,ArmorRequiresSpecificHeritage=0x586}
enum HeritageGroup{Invalid=0,Gearknight=6,Olthoi=12,OlthoiAcid=13}
enum EquipMask:uint{Clothing=0x800001ff,Armor=0x7f00}
enum PropertyInt{HeritageSpecificArmor=324} enum PropertyInstanceId{AllowedWielder=32}enum PropertyBool{Any=1}enum PropertyAttribute{Any=1}enum PropertyAttribute2nd{Any=1}enum Skill{Any=1}
class GuidId{public uint Full=123;}
class Stat{public uint Base,Current,MaxValue;public int AdvancementClass;}
static class PropertyManager{public static bool Enabled=true;public static (bool Item,bool _) GetBool(string n)=>(Enabled,false);}
class WorldObject{public int? Heritage;public uint? Allowed;public EquipMask ValidLocations;
public int? GetProperty(PropertyInt p)=>Heritage;public uint? GetProperty(PropertyInstanceId p)=>Allowed;
public WieldRequirement WieldRequirements,WieldRequirements2,WieldRequirements3,WieldRequirements4;
public int? WieldSkillType=1,WieldSkillType2=1,WieldSkillType3=1,WieldSkillType4=1;
public int? WieldDifficulty,WieldDifficulty2,WieldDifficulty3,WieldDifficulty4;}
class Player{public HeritageGroup HeritageGroup;public bool IsOlthoiPlayer=>(int)HeritageGroup==12||(int)HeritageGroup==13;public bool IsGearKnightPlayer=>(int)HeritageGroup==6;public GuidId Guid=new();public int? Level=50;public CreatureType? CreatureType=ACE.Entity.Enum.CreatureType.Human;
public Dictionary<PropertyAttribute,Stat> Attributes=new(){{(PropertyAttribute)1,new Stat{Base=50,Current=100}}};public Dictionary<PropertyAttribute2nd,Stat> Vitals=new(){{(PropertyAttribute2nd)1,new Stat{Base=50,MaxValue=100}}};
public Stat GetCreatureSkill(Skill s,bool b)=>new Stat{Base=50,Current=100,AdvancementClass=2};public Skill ConvertToMoASkill(Skill s)=>s;
public int? GetProperty(PropertyInt p)=>-1;public bool? GetProperty(PropertyBool p)=>true;
METHODS
public int Check(WorldObject item)=>(int)CheckWieldRequirements(item);}
class Program{static void Main(){
foreach(int kind in new[]{0,1,2,3,4,5,6,7,8,9,10,11,12,999})foreach(int diff in new[]{-1,0,1,2,3,31,49,50,51,99,100,101})foreach(int slot in new[]{0,1,2,3}){
var p=new Player{HeritageGroup=(HeritageGroup)3};var w=new WorldObject();
if(slot==0){w.WieldRequirements=(WieldRequirement)kind;w.WieldDifficulty=diff;}if(slot==1){w.WieldRequirements2=(WieldRequirement)kind;w.WieldDifficulty2=diff;}if(slot==2){w.WieldRequirements3=(WieldRequirement)kind;w.WieldDifficulty3=diff;}if(slot==3){w.WieldRequirements4=(WieldRequirement)kind;w.WieldDifficulty4=diff;}
Console.WriteLine($"criterion {kind} {diff} {slot} {p.Check(w)}");}
foreach(bool enabled in new[]{false,true})foreach(int heritage in new[]{3,6,12,13})foreach(int specific in new[]{-1,3,6,12,13})foreach(uint loc in new[]{0u,1u,0x4000u,0x8000u,0x80000000u})foreach(uint owner in new[]{0u,123u,124u}){
PropertyManager.Enabled=enabled;var p=new Player{HeritageGroup=(HeritageGroup)heritage};var w=new WorldObject{Heritage=specific==-1?null:specific,Allowed=owner==0?null:owner,ValidLocations=(EquipMask)loc};
Console.WriteLine($"policy {(enabled?1:0)} {heritage} {specific} {loc} {owner} {p.Check(w)}");}
}}
'''.replace('METHODS',methods)
with tempfile.TemporaryDirectory(prefix='betterace-wield-') as d:
 p=Path(d);(p/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>');(p/'Program.cs').write_text(code)
 out=subprocess.check_output([os.environ.get('DOTNET','/tmp/bace-crafting-dotnet/dotnet'),'run','--project',str(p/'Oracle.csproj'),'-c','Release'],text=True)
 rows='\n'.join(l for l in out.splitlines() if l.startswith(('criterion ','policy ')))+'\n'
 (ROOT/'crates/gameplay/bace-inventory/tests/fixtures/wield.txt').write_text('# ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b Player_Inventory.cs sha256 '+hashlib.sha256(a.encode()).hexdigest()+'\n'+rows)
 print(len(rows.splitlines()),'original ACE vectors')
