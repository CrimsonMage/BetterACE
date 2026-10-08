#!/usr/bin/env python3
"""Original pinned ACE property/message/script serializers; no Rust dependency."""
from pathlib import Path
import tempfile, subprocess, hashlib
ROOT=Path(__file__).resolve().parents[4]
SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
N=SRC/'ACE.Server/Network'
def method(text,signature):
 start=text.index(signature);a=text.index('{',start);depth=1;b=a+1
 while depth:
  depth+=(text[b]=='{')-(text[b]=='}');b+=1
 return text[start:b]
files=[N/f'GameMessages/Messages/GameMessage{name}.cs' for name in ['PrivateUpdatePropertyInt64','SystemChat','Script']]
files += [N/'GameMessages/GameMessageOpcode.cs',SRC/'ACE.Entity/Enum/PlayScript.cs',SRC/'ACE.Entity/Enum/ChatMessageType.cs',SRC/'ACE.Entity/Enum/Properties/PropertyInt64.cs']
files += [SRC/'ACE.Entity/Enum/SquelchMask.cs',SRC/'ACE.Entity/Enum/Properties/SendOnLoginAttribute.cs',SRC/'ACE.Entity/Enum/Properties/AssessmentPropertyAttribute.cs']
ext=N/'Extensions.cs';source=ext.read_text();methods='\n'.join(method(source,s) for s in ['private static uint CalculatePadMultiple','public static void WriteString16L','public static void Pad(this BinaryWriter','public static void WriteGuid'])
harness=r'''
using ACE.Entity;using System;using System.IO;using System.Text;using ACE.Server.Network.GameMessages.Messages;using ACE.Entity.Enum;using ACE.Entity.Enum.Properties;
class Program{static void Main(){Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);foreach(byte seq in new byte[]{0,1,255})foreach(long xp in new long[]{0,123456789,long.MaxValue}){var item=new ACE.Server.WorldObjects.WorldObject();item.Sequences.Value=seq;Emit("xp,"+seq+","+xp,new GameMessagePrivateUpdatePropertyInt64(item,PropertyInt64.ItemTotalXp,xp));}Emit("chat,0,0",new GameMessageSystemChat("Your Aetheria has increased in power to level 3!",ChatMessageType.Broadcast));Emit("script,0,0",new GameMessageScript(new ACE.Entity.ObjectGuid(17),PlayScript.AetheriaLevelUp));}static void Emit(string name,ACE.Server.Network.GameMessages.GameMessage m)=>Console.WriteLine(name+","+Convert.ToHexString(m.Data.ToArray()));}
namespace ACE.Entity{public struct ObjectGuid{public uint Full;public ObjectGuid(uint x){Full=x;}}}
namespace ACE.Server.Network{public enum GameMessageGroup{UIQueue,SmartboxQueue}public static class Extensions{METHODS}}
namespace ACE.Server.Network.Sequence{public enum SequenceType{UpdatePropertyInt64}}
namespace ACE.Server.WorldObjects{public class WorldObject{public SequenceManager Sequences=new();}public class SequenceManager{public byte Value;public byte GetNextSequence(ACE.Server.Network.Sequence.SequenceType type,PropertyInt64 property)=>Value;}}
namespace ACE.Server.Network.GameMessages{public class GameMessage{public MemoryStream Data=new();public BinaryWriter Writer;public GameMessage(GameMessageOpcode op,ACE.Server.Network.GameMessageGroup group,int capacity=0){Writer=new(Data);Writer.Write((uint)op);}}}
'''.replace('METHODS',methods)
with tempfile.TemporaryDirectory(prefix='item-xp-oracle-') as tmp:
 p=Path(tmp)
 for i,f in enumerate(files):(p/f'source{i}.cs').write_bytes(f.read_bytes())
 (p/'Program.cs').write_text(harness)
 (p/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 subprocess.run(['/tmp/bace-crafting-dotnet/dotnet','build','--nologo','-o',str(p/'build')],cwd=p,check=True)
 output=subprocess.check_output(['/tmp/bace-crafting-dotnet/dotnet',str(p/'build/oracle.dll')],text=True)
 target=Path(__file__).resolve().parents[1]/'tests/fixtures/item_experience.csv'
 target.write_text(''.join('# '+str(f.relative_to(SRC))+' '+hashlib.sha256(f.read_bytes()).hexdigest()+'\n' for f in files+[ext])+output)
