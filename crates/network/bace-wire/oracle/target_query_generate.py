#!/usr/bin/env python3
"""Compile original event constructors and original base event header writer."""
from pathlib import Path
import hashlib,re,subprocess,tempfile,os
ROOT=Path(__file__).resolve().parents[4]
BASE=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Server/Network'
def method(text,marker):
 s=text.index(marker);e=text.index('{',s)+1;depth=1
 while depth:
  if text[e]=='{':depth+=1
  elif text[e]=='}':depth-=1
  e+=1
 return text[s:e]
base=(BASE/'GameEvent/GameEventMessage.cs').read_text()
header=method(base,'        protected GameEventMessage(GameEventType eventType, GameMessageGroup group, Session session, int dataInitialCapacity)')
events={name:(BASE/f'GameEvent/Events/GameEvent{name}.cs').read_text() for name in ['UpdateHealth','QueryItemManaResponse']}
enums=(BASE/'GameEvent/GameEventType.cs').read_text()
values={name:re.search(r'\b'+name+r'\s*=\s*(0x[\dA-Fa-f]+)',enums).group(1) for name in events}
code='''using System;using System.IO;
class ObjectGuid {public uint Full;public ObjectGuid(uint value){Full=value;}}
class Player {public ObjectGuid Guid=new(0x50000001);}class Session {public Player Player=new();public uint GameEventSequence=42;}
static class Ext {public static void WriteGuid(this BinaryWriter w,ObjectGuid id){w.Write(id.Full);}}
enum GameMessageGroup {UIQueue=9}enum GameMessageOpcode {GameEvent=0xf7b0}
class GameMessage {protected MemoryStream Data=new();protected BinaryWriter Writer;public GameMessage(GameMessageOpcode op,GameMessageGroup group,int capacity){Writer=new(Data);Writer.Write((uint)op);}public byte[] Bytes()=>Data.ToArray();}
'''+ 'enum GameEventType {'+','.join(n+'='+v for n,v in values.items())+'}\n'+'''class GameEventMessage:GameMessage {GameEventType EventType;Session Session;
'''+header+'}\n'
for name,text in events.items():
 code+=f'class GameEvent{name}:GameEventMessage {{'+method(text,f'        public GameEvent{name}(')+'}\n'
code+='''class Program {static void Main(){foreach(uint bits in new uint[]{0,0x3f000000,0x3f800000,0x7f800000,0xffc00000})foreach(uint target in new uint[]{0,0xfedcba98}){float value=BitConverter.Int32BitsToSingle((int)bits);Console.WriteLine($"H|{target}|{bits:X8}|0|{Convert.ToHexString(new GameEventUpdateHealth(new(),target,value).Bytes())}");foreach(uint success in new uint[]{0,1})Console.WriteLine($"M|{target}|{bits:X8}|{success}|{Convert.ToHexString(new GameEventQueryItemManaResponse(new(),target,value,success).Bytes())}");}}}'''
with tempfile.TemporaryDirectory(prefix='bace-query-packets-') as directory:
 p=Path(directory);(p/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>');(p/'Program.cs').write_text(code)
 result=subprocess.check_output([os.environ.get('DOTNET','/tmp/bace-crafting-dotnet/dotnet'),'run','--project',str(p/'Oracle.csproj'),'-c','Release'],text=True)
 rows=[r for r in result.splitlines() if r.startswith(('H|','M|'))];assert len(rows)==30
 provenance='\n'.join('# '+name+' '+hashlib.sha256(text.encode()).hexdigest() for name,text in {'GameEventMessage':base,**events}.items())
 (ROOT/'crates/network/bace-wire/tests/fixtures/target_query.txt').write_text('# ACE47edade3bd3f6044b676d4eb877c4965c7eda62b\n'+provenance+'\n'+'\n'.join(rows)+'\n');print(len(rows),'original query event packets')
