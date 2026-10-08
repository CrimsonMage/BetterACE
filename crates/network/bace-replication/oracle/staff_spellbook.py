#!/usr/bin/env python3
"""Original ACE LearnSpellWithNetworking/HandleRemoveSpell and message encoders."""
from pathlib import Path
import subprocess,tempfile,hashlib
ROOT=Path(__file__).resolve().parents[4];SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source';N=SRC/'ACE.Server/Network'
def method(text,signature):
 a=text.index(signature);i=text.index('{',a)+1;d=1
 while d:d+=(text[i]=='{')-(text[i]=='}');i+=1
 return text[a:i]
player=SRC/'ACE.Server/WorldObjects/Player_Spells.cs';admin=SRC/'ACE.Server/Command/Handlers/AdminCommands.cs';ext=N/'Extensions.cs'
methods='\n'.join(method(ext.read_text(),sig)for sig in ['private static uint CalculatePadMultiple','public static void WriteString16L','public static void Pad(this BinaryWriter','public static void WriteGuid'])
files=[N/'GameEvent/GameEventMessage.cs',N/'GameEvent/GameEventType.cs',N/'GameEvent/Events/GameEventMagicUpdateSpell.cs',N/'GameMessages/GameMessageOpcode.cs']+[N/('GameMessages/Messages/GameMessage'+v+'.cs')for v in ['SystemChat','Script']]+[SRC/('ACE.Entity/Enum/'+v+'.cs')for v in ['SpellId','PlayScript','ChatMessageType','SquelchMask']]
h=r'''
using System;using System.IO;using System.Text;using System.Collections.Generic;using ACE.Entity;using ACE.Entity.Enum;using ACE.Server.Network;using ACE.Server.Network.GameMessages;using ACE.Server.Network.GameMessages.Messages;using ACE.Server.Network.GameEvent.Events;
class Program{public static int Case;static void Main(){Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);for(Case=0;Case<4;Case++){var p=new ACE.Server.WorldObjects.Player{Known=Case==1||Case==2};if(Case<2)p.LearnSpellWithNetworking(20);else ACE.Server.Command.Handler.HandleRemoveSpell(p.Session,"20");}}}
class DatManager{public static Portal PortalDat=new();}class Portal{public Table SpellTable=new();}class Table{public Dictionary<uint,Definition>Spells=new(){{20,new Definition()}};}class Definition{public string Name="Test Spell";}
namespace ACE.Entity{public struct ObjectGuid{public uint Full;public ObjectGuid(uint id){Full=id;}}}
namespace ACE.Server.Entity{public class Spell{public string Name="Test Spell";public Spell(SpellId id,bool check){}}}
namespace ACE.Server.Command{public class Handler{REMOVE}}
namespace ACE.Server.WorldObjects{public class Player{public ObjectGuid Guid=new(17);public Session Session;public bool Known;public Player(){Session=new Session{Player=this};}public bool AddKnownSpell(uint id){if(Known)return false;Known=true;return true;}public bool RemoveKnownSpell(uint id){if(!Known)return false;Known=false;return true;}public void ApplyVisualEffects(PlayScript script)=>Session.Network.EnqueueSend(new GameMessageScript(Guid,script));LEARN}}
namespace ACE.Server.Network{public enum GameMessageGroup{UIQueue=9,SmartboxQueue=10}public class Session{public ACE.Server.WorldObjects.Player Player;public uint GameEventSequence=42;public Network Network=new();}public class Network{public void EnqueueSend(GameMessage m)=>Console.WriteLine(Program.Case+","+(int)m.Group+","+Convert.ToHexString(m.Data.ToArray()));}public static class Extensions{METHODS}}
namespace ACE.Server.Network.GameMessages{public class GameMessage{public MemoryStream Data=new();public BinaryWriter Writer;public GameMessageGroup Group;public GameMessage(GameMessageOpcode op,GameMessageGroup group,int capacity=0){Group=group;Writer=new(Data);Writer.Write((uint)op);}}}
namespace ACE.Server.Network.GameEvent.Events{public class GameEventCommunicationTransientString:GameMessage{public GameEventCommunicationTransientString(Session session,string text):base(GameMessageOpcode.GameEvent,GameMessageGroup.UIQueue){throw new Exception("uiOutput=false is outside this oracle fixture");}}}
'''.replace('METHODS',methods).replace('LEARN',method(player.read_text(),'public void LearnSpellWithNetworking(')).replace('REMOVE',method(admin.read_text(),'public static void HandleRemoveSpell('))
with tempfile.TemporaryDirectory(prefix='staff-spellbook-') as tmp:
 p=Path(tmp);(p/'Program.cs').write_text(h)
 for i,f in enumerate(files):(p/f'source{i}.cs').write_bytes(f.read_bytes())
 (p/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 subprocess.run(['/tmp/bace-crafting-dotnet/dotnet','build','--nologo','-o',str(p/'build')],cwd=p,check=True)
 out=subprocess.check_output(['/tmp/bace-crafting-dotnet/dotnet',str(p/'build/oracle.dll')],text=True)
 Path(__file__).parents[1].joinpath('tests/fixtures/staff_spellbook.csv').write_text(''.join('# '+str(v.relative_to(SRC))+' '+hashlib.sha256(v.read_bytes()).hexdigest()+'\n' for v in files+[player,admin,ext])+out)
