#!/usr/bin/env python3
"""Compile pinned ACE UI action handlers; record consumed bytes and domain calls."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile

PIN = "47edade3bd3f6044b676d4eb877c4965c7eda62b"
HERE = Path(__file__).resolve().parent
BASE = "Source/ACE.Server/Network/"
ACTIONS = ["SetCharacterOptions", "SetSingleCharacterOption", "AddShortcut", "RemoveShortcut", "AddSpellFavorite", "RemoveSpellFavorite", "SpellbookFilter", "SetDesiredComponentLevel"]
SOURCES = [BASE + "GameAction/Actions/GameAction" + action + ".cs" for action in ACTIONS] + [
    BASE + "Structure/Shortcut.cs", BASE + "Structure/LayeredSpell.cs",
    "Source/ACE.Entity/Enum/CharacterOptionDataFlag.cs", BASE + "Enum/GenericQualitiesPackHeader.cs",
]
HARNESS = r'''
using System;using System.IO;using System.Collections.Generic;
using ACE.Server.Network;using ACE.Server.Network.GameAction.Actions;
public class Program {
 public static void Main(){
  Test("single",5,Words(3,2,0xfedcba98),GameActionSetSingleCharacterOption.Handle);
  Test("shortcut",0x19c,Words(17,0xf0100203,0x00080064),GameActionAddShortcut.Handle);
  Test("remove_shortcut",0x19d,Words(17),GameActionRemoveShortcut.Handle);
  Test("add_favorite",0x1e3,Words(100,2,7),GameActionAddSpellFavorite.Handle);
  Test("remove_favorite",0x1e4,Words(100,7),GameActionRemoveSpellFavorite.Handle);
  Test("filters",0x286,Words(0x11112222),GameActionSpellbookFilter.Handle);
  Test("component",0x224,Words(5001,40),GameActionSetDesiredComponentLevel.Handle);
  Test("component_bits",0x224,Words(5001,0xffffffff),GameActionSetDesiredComponentLevel.Handle);
  Test("options_empty",0x1a1,Words(0,0xaabbccdd,0),GameActionSetCharacterOptions.Handle);
  Test("options_five",0x1a1,Words(4,1,1,123,0,0,0,0),GameActionSetCharacterOptions.Handle);
  Test("options_seven",0x1a1,Words(16,1,1,123,0,0,0,0,0,0),GameActionSetCharacterOptions.Handle);
  var words=new List<uint>{0x669,0xaabbccdd,1,17,0xf0100203,0x00080064,2,123,456};
  for(uint i=0;i<7;i++){words.Add(1);words.Add(1000+i);}
  words.AddRange(new uint[]{0x00200002,5001,40,5002,0xffffffff,0x11112222,0xdeadbeef});
  var bytes=new List<byte>(Words(words.ToArray()));bytes.AddRange(new byte[]{0xde,0xad,0xbe,0xef,0});
  Test("options_full",0x1a1,bytes.ToArray(),GameActionSetCharacterOptions.Handle);
 }
 static byte[] Words(params uint[] words){using var stream=new MemoryStream();using var writer=new BinaryWriter(stream);foreach(var word in words)writer.Write(word);return stream.ToArray();}
 static void Test(string name,uint opcode,byte[] bytes,Action<ClientMessage,Session> handler){
  var message=new ClientMessage{Payload=new BinaryReader(new MemoryStream(bytes))};var session=new Session();handler(message,session);
  Console.WriteLine(name+","+opcode+","+Convert.ToHexString(bytes).ToLowerInvariant()+","+message.Payload.BaseStream.Position+","+string.Join("|",session.Player.Calls));
 }
}
namespace ACE.Common.Extensions {public static class Readers {
 public static string ReadString16L(this BinaryReader r)=>throw new Exception("Unsupported timestamp fixture");
 public static void Skip(this BinaryReader r,int n)=>r.BaseStream.Seek(n,SeekOrigin.Current);
}}
namespace ACE.Entity.Enum {
 public enum SpellBookFilterOptions:uint {}
 public enum CharacterOption {AppearOffline=100,AutomaticallyAcceptFellowshipRequests,IgnoreFellowshipRequests,ShowYourCloak,ShowYourHelmOrHeadGear,ListenToAllegianceChat,ListenToGeneralChat,ListenToLFGChat,ListenToRoleplayChat,ListenToSocietyChat,ListenToTradeChat}
}
namespace ACE.Entity.Models {public class PropertiesEnchantmentRegistry {public int SpellId;public ushort LayerId;}}
namespace ACE.Database.Models.Shard {public class CharacterPropertiesShortcutBar {public uint ShortcutBarIndex,ShortcutObjectId;}}
namespace log4net {public interface ILog{void Warn(string s);}public class Logger:ILog{public void Warn(string s){}} public static class LogManager{public static ILog GetLogger(Type t)=>new Logger();}}
namespace ACE.Server.Network.GameMessages.Messages {public class GameMessageObjDescEvent{public GameMessageObjDescEvent(object p){}}}
namespace ACE.Server.Network.GameAction {
 public enum GameActionType{SetCharacterOptions,SetSingleCharacterOption,AddShortCut,RemoveShortCut,AddSpellFavorite,RemoveSpellFavorite,SpellbookFilter,SetDesiredComponentLevel}
 public class GameActionAttribute:Attribute{public GameActionAttribute(GameActionType t){}}
}
namespace ACE.Server.Network {
 public class ClientMessage{public BinaryReader Payload;}
 public class Session{public Player Player=new();}
 public class Player{
  public bool FirstEnterWorldDone=true;public string Name="Fixture";public List<string> Calls=new();
  public void SetCharacterOptions1(int v)=>Calls.Add("options1="+unchecked((uint)v));
  public void SetCharacterOptions2(int v)=>Calls.Add("options2="+unchecked((uint)v));
  public void SetCharacterGameplayOptions(byte[] v)=>Calls.Add("gameplay="+Convert.ToHexString(v).ToLowerInvariant());
  public void HandleActionAddShortcut(Structure.Shortcut s)=>Calls.Add($"shortcut={s.Index}:{s.ObjectId}:{s.Spell.SpellId}:{s.Spell.Layer}");
  public void HandleActionRemoveShortcut(uint v)=>Calls.Add("remove="+v);
  public void HandleActionAddSpellFavorite(uint s,uint p,uint b)=>Calls.Add($"favorite={s}:{p}:{b}");
  public void HandleActionRemoveSpellFavorite(uint s,uint b)=>Calls.Add($"unfavorite={s}:{b}");
  public void HandleSpellbookFilters(ACE.Entity.Enum.SpellBookFilterOptions v)=>Calls.Add("filters="+(uint)v);
  public void HandleSetDesiredComponentLevel(uint s,uint q)=>Calls.Add($"component={s}:{q}");
  public void SetCharacterOption(ACE.Entity.Enum.CharacterOption o,bool b)=>Calls.Add($"option={(uint)o}:{b.ToString().ToLowerInvariant()}");
  public void SetAppearOffline(bool b)=>throw new Exception("Unexpected policy fixture");
  public void EnqueueBroadcast(object o){} public void JoinTurbineChatChannel(string s){}public void LeaveTurbineChatChannel(string s){}
 }
}
'''
parser = argparse.ArgumentParser()
parser.add_argument("--source",type=Path,required=True)
parser.add_argument("--dotnet",default="dotnet")
args=parser.parse_args()
hashes=[]
with tempfile.TemporaryDirectory(prefix="bace-ui-") as directory:
    work=Path(directory)
    for index,relative in enumerate(SOURCES):
        data=(args.source/relative).read_bytes()
        hashes.append(f"# {relative} sha256={hashlib.sha256(data).hexdigest()}")
        (work/f"Source{index}.cs").write_bytes(data)
    (work/"Program.cs").write_text(HARNESS)
    (work/"Oracle.csproj").write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>')
    subprocess.run([args.dotnet,"build","--nologo","-o",str(work/"out"),str(work/"Oracle.csproj")],check=True)
    output=subprocess.check_output([args.dotnet,str(work/"out/Oracle.dll")],text=True)
    path=HERE.parent/"tests/fixtures/ui_input.csv"
    path.parent.mkdir(parents=True,exist_ok=True)
    path.write_text(f"# Official ACEmulator/ACE {PIN}; AGPL-3.0-only; ACE contributors\n"+"\n".join(hashes)+"\n# Original handlers/shortcut decoders; synthetic domain-call recorder. Policy branch enum stubs do not establish option side-effect parity.\n"+output)
