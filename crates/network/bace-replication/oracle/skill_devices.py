#!/usr/bin/env python3
"""Execute original device output methods with synthetic state/transport boundaries.
The trace proves source order/notice selection. Existing wire fixtures establish
serializer bytes; skill-transition fixtures establish numeric gameplay results.
"""
from pathlib import Path
import hashlib, subprocess, tempfile
ROOT=Path(__file__).resolve().parents[4];PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
SRC=ROOT/'.reference'/('ACE-'+PIN)/'Source'
paths=['ACE.Server/WorldObjects/SkillAlterationDevice.cs','ACE.Server/WorldObjects/AugmentationDevice.cs','ACE.Server/WorldObjects/Player_Skills.cs','ACE.Server/WorldObjects/Player_Xp.cs','ACE.Entity/Enum/Skill.cs','ACE.Entity/Enum/WeenieErrorWithString.cs']
sources={p:(SRC/p).read_text() for p in paths}
def method(path,signature):
    s=sources[path];a=s.index(signature);i=s.index('{',a)+1;depth=1
    while depth:depth+=(s[i]=='{')-(s[i]=='}');i+=1
    return s[a:i]
player='\n'.join(method(paths[2],s) for s in ['public bool SpecializeSkill(Skill skill, int creditsSpent','public bool UntrainSkill(','public bool UnspecializeSkill('])+ '\n'+method(paths[3],'public void RefundXP(')
program=r'''using System;using System.Collections.Generic;using ACE.Entity.Enum;
enum SkillAdvancementClass{Untrained=1,Trained=2,Specialized=3}
enum PropertyInt{AvailableSkillCredits=24,ContainersCapacity=7,EncumbranceCapacity=38,Salvage=224,Item=225,Armor=226,MagicItem=227,Weapon=228}
enum PropertyInt64{AvailableExperience=2} enum ChatMessageType{Broadcast=0}
enum AugmentationType{Salvage=7,ItemTinkering=8,ArmorTinkering=9,MagicItemTinkering=10,WeaponTinkering=11,PackSlot=12,BurdenLimit=13}
static class AugTypeHelper{public static bool IsAttribute(AugmentationType t)=>false;public static bool IsResist(AugmentationType t)=>false;public static bool IsSkill(AugmentationType t)=>true;public static int GetAttribute(AugmentationType t)=>0;public static Skill GetSkill(AugmentationType t)=>t switch{AugmentationType.Salvage=>Skill.Salvaging,AugmentationType.ItemTinkering=>Skill.ItemTinkering,AugmentationType.ArmorTinkering=>Skill.ArmorTinkering,AugmentationType.MagicItemTinkering=>Skill.MagicItemTinkering,_=>Skill.WeaponTinkering};public static uint GetEffect(AugmentationType t)=>0x9d;}
class CreatureSkill{public Skill Skill;public SkillAdvancementClass AdvancementClass;public ushort Ranks=4;public uint ExperienceSpent=50,InitLevel;}
class SkillBase{public int UpgradeCostFromTrainedToSpecialized=2,TrainedCost=4;}
class Attr{public uint StartingValue;}
class Msg{public string Text;public Msg(string text){Text=text;}}
class GameMessagePrivateUpdateSkill:Msg{public GameMessagePrivateUpdateSkill(Player p,CreatureSkill s):base($"skill/{(int)s.Skill}") {}}
class GameMessagePrivateUpdatePropertyInt:Msg{public GameMessagePrivateUpdatePropertyInt(Player p,PropertyInt prop,int value):base($"int/{(int)prop}/{value}") {}}
class GameMessagePrivateUpdatePropertyInt64:Msg{public GameMessagePrivateUpdatePropertyInt64(Player p,PropertyInt64 prop,long value):base($"int64/{(int)prop}/{value}") {}}
class GameEventWeenieErrorWithString:Msg{public GameEventWeenieErrorWithString(Session s,WeenieErrorWithString error,string text):base($"notice/{(uint)error}/{text}") {}}
class GameMessageScript:Msg{public GameMessageScript(uint actor,uint script):base($"script/{script}") {}}
class GameMessageSystemChat:Msg{public GameMessageSystemChat(string text,ChatMessageType t):base($"chat/{text}") {}}
class GameMessagePrivateUpdateAttribute:Msg{public GameMessagePrivateUpdateAttribute(Player p,Attr a):base("unexpected attribute") {}}
class Network{public List<string>Trace=new();public void EnqueueSend(params Msg[] messages){foreach(var m in messages)Trace.Add(m.Text);}}
class Session{public Network Network=new();}
class Player{public string Name="Alice";public uint Guid=0x50000001;public Session Session=new();public CreatureSkill Skill;public bool Augmented;
public int? AvailableSkillCredits=12;public long? AvailableExperience=100;public Dictionary<int,Attr>Attributes=new();public int AugmentationInnateFamily,AugmentationResistanceFamily;public uint ContainerCapacity;
public int? GetProperty(PropertyInt p)=>0;public void SetProperty(PropertyInt p,int value){}public int GetEncumbranceCapacity()=>0;
public static uint CalcSkillRank(SkillAdvancementClass c,uint xp)=>6;
public CreatureSkill GetCreatureSkill(Skill skill)=>Skill;
public bool IsSkillSpecializedViaAugmentation(Skill skill,out bool has){has=Augmented;return Augmented;}
public static bool IsSkillUntrainable(Skill skill)=>skill!=ACE.Entity.Enum.Skill.ArcaneLore;
public void TryConsumeFromInventoryWithNetworking(object device,int amount){Session.Network.Trace.Add("consume");}
public void SendWeenieErrorWithString(WeenieErrorWithString code,string text){Session.Network.EnqueueSend(new GameEventWeenieErrorWithString(Session,code,text));}
public void EnqueueBroadcast(Msg m){Session.Network.EnqueueSend(m);}public void SaveBiotaToDatabase(){}
PLAYER
}
class Alteration{public enum SkillAlterationType{Specialize=1,Lower=2}public SkillAlterationType TypeOfAlteration;
ALTER
}
class Augmentation{public string Name="Test Gem";public int? AugmentationStat;public long? AugmentationCost=25;public static bool AttributeAugmentationSafetyCapEnabled=false;
public Dictionary<AugmentationType,PropertyInt>AugProps=new(){{AugmentationType.Salvage,PropertyInt.Salvage},{AugmentationType.ItemTinkering,PropertyInt.Item},{AugmentationType.ArmorTinkering,PropertyInt.Armor},{AugmentationType.MagicItemTinkering,PropertyInt.MagicItem},{AugmentationType.WeaponTinkering,PropertyInt.Weapon}};
AUG
}
class Program{static void Main(){for(int mode=0;mode<10;mode++){int id=mode>=5?new[]{40,18,29,30,28}[mode-5]:mode==2?28:mode==4?14:44;var p=new Player{Augmented=mode==2,Skill=new CreatureSkill{Skill=(Skill)id,AdvancementClass=(mode==1||mode==2)?SkillAdvancementClass.Specialized:SkillAdvancementClass.Trained}};if(mode<5)new Alteration{TypeOfAlteration=mode==0?Alteration.SkillAlterationType.Specialize:Alteration.SkillAlterationType.Lower}.AlterSkill(p,p.Skill,new SkillBase());else new Augmentation{AugmentationStat=mode+2}.DoAugmentation(p);Console.WriteLine($"{mode}|{id}|{string.Join(";",p.Session.Network.Trace)}");}}}
'''.replace('PLAYER',player).replace('ALTER',method(paths[0],'public void AlterSkill(')).replace('AUG',method(paths[1],'public void DoAugmentation('))
with tempfile.TemporaryDirectory(prefix='skill-device-output-') as tmp:
    d=Path(tmp);(d/'Program.cs').write_text(program)
    for p in paths[-2:]:(d/Path(p).name).write_text(sources[p])
    (d/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><WarningLevel>0</WarningLevel></PropertyGroup></Project>')
    dotnet='/tmp/bace-crafting-dotnet/dotnet';subprocess.run([dotnet,'build','--nologo','-o',str(d/'build')],cwd=d,check=True)
    output=subprocess.check_output([dotnet,str(d/'build/oracle.dll')],text=True)
    dest=Path(__file__).resolve().parents[1]/'tests/fixtures/skill_devices.trace'
    dest.write_text('# official ACE '+PIN+'\n'+''.join('# '+p+' sha256='+hashlib.sha256((SRC/p).read_bytes()).hexdigest()+'\n' for p in paths)+output)
    print('original-source device output traces:',len(output.splitlines()))
