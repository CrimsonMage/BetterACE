#!/usr/bin/env python3
"""Compile unchanged pinned ACE CheckUseRequirements; stub only surrounding owner getters."""
from pathlib import Path
import subprocess,tempfile,os,hashlib
ROOT=Path(__file__).resolve().parents[5]
SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Server/WorldObjects/WorldObject_Use.cs'
a=SRC.read_text();start=a.rfind('\n',0,a.index('public virtual ActivationResult CheckUseRequirements('))+1
begin=a.index('{',start);depth=1;end=begin+1
while depth:
 if a[end]=='{':depth+=1
 elif a[end]=='}':depth-=1
 end+=1
method=a[start:end]
code='''using System;using System.Collections.Generic;using ACE.Entity.Enum;
namespace ACE.Entity.Enum{enum CreatureType{Invalid,Olthoi}}
enum Skill{ArcaneLore=14,Test=54}enum SkillAdvancementClass{Inactive,Untrained,Trained,Specialized}
enum HeritageGroup{Invalid=0,Sho=3,Olthoi=12,OlthoiAcid=13}
enum WeenieErrorWithString{Your_IsTooLowToUseItemMagic=0x4c9,YouMustSpecialize_ToUseItemMagic=0x4cb,YouMustBe_ToUseItemMagic=0x4c6,_CowersFromYou=0x58a}
enum WeenieError{OlthoiCannotInteractWithThat=0x587,OlthoiCannotUseLifestones=0x588,OlthoiVendorLooksInHorror=0x589}
static class Extension{public static string ToSentence(this Enum e)=>Convert.ToInt32(e).ToString();}
class Message{public string Value;public Message(string s){Value=s;}}
class GameEventWeenieErrorWithString:Message{public GameEventWeenieErrorWithString(Session s,WeenieErrorWithString e,string v):base($"error:{(int)e}:{v}") {}}
class GameEventCommunicationTransientString:Message{public GameEventCommunicationTransientString(Session s,string v):base("transient:"+v){}}
class ActivationResult{public bool Allowed;public Message Message;public ActivationResult(bool a){Allowed=a;}public ActivationResult(Message m){Message=m;}}
class Network{public Message Last;public void EnqueueSend(Message m){Last=m;}}
class Session{public Network Network=new();}
class Stat{public uint Current,MaxValue;public SkillAdvancementClass AdvancementClass;public Skill Skill=Skill.Test;public int Attribute=1,Vital=1;}
class EnchantmentManager{public bool Ready=true;public bool CheckCooldown(int? i)=>i==null||Ready;}
class Log{public void Error(string s){}}
class WorldObject{
public static Log log=new();public string Guid="1",Name="target";public uint WeenieClassId=1;
public int? ItemDifficulty,ItemSkillLevelLimit,UseRequiresSkill,UseRequiresSkillLevel,UseRequiresSkillSpec,UseRequiresLevel,ItemAttributeLimit,ItemAttributeLevelLimit,ItemAttribute2ndLimit,ItemAttribute2ndLevelLimit,CooldownId;
public uint? ItemSkillLimit,ItemSpecializedOnly;public HeritageGroup HeritageGroup;public CreatureType CreatureType;public bool? NpcLooksLikeObject;
public Skill ConvertToMoASkill(Skill s)=>s;
METHOD
}
class Creature:WorldObject{} class Vendor:Creature{} class Lifestone:WorldObject{} class Container:WorldObject{} class Corpse:Container{}
class AttributeTransferDevice:WorldObject{} class AugmentationDevice:WorldObject{} class Bindstone:WorldObject{} class Book:WorldObject{} class Game:WorldObject{} class Gem:WorldObject{} class GenericObject:WorldObject{} class Key:WorldObject{} class SkillAlterationDevice:WorldObject{}
class Player:Creature{public Session Session=new();public int? Level=50;public bool IsOlthoiPlayer=>(int)HeritageGroup==12||(int)HeritageGroup==13;public EnchantmentManager EnchantmentManager=new();public uint Current=100;public int Training=2;
public Dictionary<int,Stat> Attributes=new(){{1,new Stat{Current=100,Attribute=1}}};public Dictionary<int,Stat> Vitals=new(){{1,new Stat{MaxValue=100,Vital=1}}};
public Stat GetCreatureSkill(Skill s)=>new Stat{Current=Current,AdvancementClass=(SkillAdvancementClass)Training,Skill=s};
public void SendWeenieError(WeenieError e){Session.Network.Last=new Message($"error:{(int)e}");}public void SendWeenieErrorWithString(WeenieErrorWithString e,string s){Session.Network.Last=new Message($"error:{(int)e}:{s}");}}
class Program{static string Check(WorldObject w,Player p){var r=w.CheckUseRequirements(p);return r.Allowed?"ok":(r.Message??p.Session.Network.Last)?.Value??"rejected";}
static void Main(){
foreach(int mode in new[]{0,1,2,3,4,5,6,7,8,9,10,11,12})foreach(int difficulty in new[]{-1,0,99,100,101})foreach(int training in new[]{0,1,2,3}){
var p=new Player{HeritageGroup=(HeritageGroup)3,Training=training};WorldObject w=new();
switch(mode){case 0:w.ItemDifficulty=difficulty;break;case 1:w.ItemSkillLimit=54;w.ItemSkillLevelLimit=difficulty;break;case 2:w.UseRequiresSkill=54;w.UseRequiresSkillLevel=difficulty;break;case 3:w.UseRequiresSkillSpec=54;w.UseRequiresSkillLevel=difficulty;break;case 4:w.ItemSpecializedOnly=54;w.ItemSkillLevelLimit=difficulty;break;case 5:w.UseRequiresLevel=difficulty;break;case 6:w.ItemAttributeLimit=1;w.ItemAttributeLevelLimit=difficulty;break;case 7:w.ItemAttribute2ndLimit=1;w.ItemAttribute2ndLevelLimit=difficulty;break;case 8:w.UseRequiresSkill=54;break;case 9:w.ItemSkillLimit=54;break;case 10:w.ItemAttributeLimit=1;break;case 11:w.ItemAttribute2ndLimit=1;break;case 12:w.ItemDifficulty=101;w.UseRequiresLevel=101;w.CooldownId=1;p.EnchantmentManager.Ready=false;break;}
Console.WriteLine($"requirement|{mode}|{difficulty}|{training}|{Check(w,p)}");}
foreach(int heritage in new[]{3,12,13})foreach(int objectKind in new[]{0,1,2,3,4,5,6,7})foreach(int required in new[]{0,3,12})foreach(bool cooldown in new[]{false,true}){
var p=new Player{HeritageGroup=(HeritageGroup)heritage};p.EnchantmentManager.Ready=cooldown;WorldObject w=objectKind switch{1=>new Creature{CreatureType=CreatureType.Olthoi},2=>new Vendor(),3=>new Creature{NpcLooksLikeObject=true},4=>new Creature(),5=>new Lifestone(),6=>new Container(),7=>new Corpse(),_=>new WorldObject()};w.HeritageGroup=(HeritageGroup)required;w.CooldownId=1;
Console.WriteLine($"object|{heritage}|{objectKind}|{required}|{(cooldown?1:0)}|{Check(w,p)}");}
}}
'''.replace('METHOD',method)
# int stubs expose ToSentence too, leaving source body itself unchanged.
code=code.replace('static class Extension{','static class Extension{public static string ToSentence(this int e)=>e.ToString();')
with tempfile.TemporaryDirectory(prefix='betterace-activation-') as d:
 p=Path(d);(p/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>');(p/'Program.cs').write_text(code)
 out=subprocess.check_output([os.environ.get('DOTNET','/tmp/bace-crafting-dotnet/dotnet'),'run','--project',str(p/'Oracle.csproj'),'-c','Release'],text=True)
 rows='\n'.join(l for l in out.splitlines() if l.startswith(('requirement|','object|')))+'\n'
 (ROOT/'crates/gameplay/bace-inventory/tests/fixtures/activation.txt').write_text('# ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b WorldObject_Use.cs sha256 '+hashlib.sha256(a.encode()).hexdigest()+'\n'+rows)
 print(len(rows.splitlines()),'original ACE vectors')
