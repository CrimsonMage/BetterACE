#!/usr/bin/env python3
"""Compile original ACE XP/level methods and serializers; no Rust dependency."""
from pathlib import Path
import tempfile,subprocess,hashlib
ROOT=Path(__file__).resolve().parents[4];SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source';N=SRC/'ACE.Server/Network'
def method(text,signature):
 start=text.index(signature);a=text.index('{',start);depth=1;b=a+1
 while depth:depth+=(text[b]=='{')-(text[b]=='}');b+=1
 return text[start:b]
files=[N/f'GameMessages/Messages/GameMessage{name}.cs' for name in ['PrivateUpdatePropertyInt64','PrivateUpdatePropertyInt','PrivateUpdateAttribute2ndLevel','SystemChat','Script']]
files += [N/'GameMessages/GameMessageOpcode.cs']+[SRC/f'ACE.Entity/Enum/{name}.cs' for name in ['PlayScript','ChatMessageType','SquelchMask','Vital','XpType']]+[SRC/f'ACE.Entity/Enum/Properties/{name}.cs' for name in ['PropertyInt64','PropertyInt','SendOnLoginAttribute','AssessmentPropertyAttribute','EphemeralAttribute']]
ext=N/'Extensions.cs';methods='\n'.join(method(ext.read_text(),s) for s in ['private static uint CalculatePadMultiple','public static void WriteString16L','public static void Pad(this BinaryWriter','public static void WriteGuid'])
xp=SRC/'ACE.Server/WorldObjects/Player_Xp.cs';xp_methods='\n'.join(method(xp.read_text(),s) for s in ['private void UpdateXpAndLevel','private void CheckForLevelup'])
harness=r'''
using System;using System.IO;using System.Text;using System.Linq;using System.Collections.Generic;using ACE.Entity;using ACE.Entity.Enum;using ACE.Entity.Enum.Properties;using ACE.Server.Network.GameMessages;using ACE.Server.Network.GameMessages.Messages;
class Program{public static string Key="";static void Main(){Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);foreach(var c in new[]{(1,0L,50L),(1,0L,100L),(1,0L,350L),(3,300L,500L),(4,600L,999L)}){Key=$"{c.Item1},{c.Item2},{c.Item3}";var p=new ACE.Server.WorldObjects.Player{Level=c.Item1,TotalExperience=c.Item2,AvailableExperience=c.Item2,AvailableSkillCredits=0};p.Run(c.Item3);}}}
class DatManager{public static Portal PortalDat=new();}class Portal{public Table XpTable=new();}class Table{public List<ulong> CharacterLevelXPList=new(){0,0,100,300,600};public List<uint> CharacterLevelSkillCreditList=new(){0,0,0,2,0};}
namespace ACE.Entity{public struct ObjectGuid{public uint Full;public ObjectGuid(uint x){Full=x;}}}
namespace ACE.Server.Network{public enum GameMessageGroup{UIQueue,SmartboxQueue}public static class Extensions{METHODS}}
namespace ACE.Server.Network.Sequence{public enum SequenceType{UpdatePropertyInt64,UpdatePropertyInt,UpdateAttribute2ndLevel}}
namespace ACE.Server.WorldObjects{
 public class WorldObject{public SequenceManager Sequences=new();}public class SequenceManager{public byte GetNextSequence<T>(ACE.Server.Network.Sequence.SequenceType type,T property)=>0;}
 public class Player:WorldObject{public int? Level=1,AvailableSkillCredits=0,TotalSkillCredits=0;public long? TotalExperience=0,AvailableExperience=0;public Session Session=new();public Group Fellowship=null,AllegianceNode=null;public ObjectGuid Guid=new(17);bool HasVitae=>false;int GetMaxLevel()=>4;void UpdateXpVitae(long amount){}public void Run(long amount)=>UpdateXpAndLevel(amount,XpType.Quest);
 void PlayParticleEffect(PlayScript script,ObjectGuid id)=>Session.Network.EnqueueSend(new GameMessageScript(id,script));void SetMaxVitals(){foreach(var v in new[]{(Vital.Health,100u),(Vital.Stamina,80u),(Vital.Mana,60u)})Session.Network.EnqueueSend(new GameMessagePrivateUpdateAttribute2ndLevel(this,v.Item1,v.Item2));}
 XPMETHODS
 }public class Group{public void OnFellowLevelUp(Player p){}public void OnLevelUp(){}}public class Session{public Network Network=new();}public class Network{public void EnqueueSend(params GameMessage[] messages){foreach(var m in messages)Console.WriteLine(Program.Key+","+Convert.ToHexString(m.Data.ToArray()));}}
}
namespace ACE.Server.Network.GameMessages{public class GameMessage{public MemoryStream Data=new();public BinaryWriter Writer;public GameMessage(GameMessageOpcode op,ACE.Server.Network.GameMessageGroup group,int capacity=0){Writer=new(Data);Writer.Write((uint)op);}}}
'''.replace('XPMETHODS',xp_methods).replace('METHODS',methods)
with tempfile.TemporaryDirectory(prefix='character-xp-oracle-') as tmp:
 p=Path(tmp)
 for i,f in enumerate(files):
  text=f.read_text()
  if f.name=='PropertyInt.cs':text='namespace ACE.Entity.Enum.Properties{'+method(text,'public enum PropertyInt')+'}'
  (p/f'source{i}.cs').write_text(text)
 (p/'Program.cs').write_text(harness)
 (p/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 subprocess.run(['/tmp/bace-crafting-dotnet/dotnet','build','--nologo','-o',str(p/'build')],cwd=p,check=True)
 output=subprocess.check_output(['/tmp/bace-crafting-dotnet/dotnet',str(p/'build/oracle.dll')],text=True)
 (Path(__file__).resolve().parents[1]/'tests/fixtures/experience.csv').write_text(''.join('# '+str(f.relative_to(SRC))+' '+hashlib.sha256(f.read_bytes()).hexdigest()+'\n' for f in files+[ext,xp])+output)
