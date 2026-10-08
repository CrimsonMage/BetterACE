#!/usr/bin/env python3
"""Compile unchanged pinned ACE crafting serializers and action handlers."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile

PIN = "47edade3bd3f6044b676d4eb877c4965c7eda62b"
p = argparse.ArgumentParser()
p.add_argument("--repo", type=Path, required=True)
p.add_argument("--dotnet", required=True)
a = p.parse_args()
root = Path(__file__).resolve().parents[1]
paths = ["Source/ACE.Entity/Enum/" + name + ".cs" for name in ["ConfirmationType","MaterialType","Skill"]]
paths += ["Source/ACE.Server/Network/" + path for path in [
    "GameEvent/GameEventType.cs","GameAction/GameActionType.cs",
    "GameAction/Actions/GameActionCreateTinkeringTool.cs","GameAction/Actions/GameActionConfirmationResponse.cs",
    "GameEvent/Events/GameEventConfirmationRequest.cs","GameEvent/Events/GameEventConfirmationDone.cs",
    "GameEvent/Events/GameEventSalvageOperationsResult.cs","Structure/SalvageResult.cs"]]
sources = {path:subprocess.check_output(["git","-C",str(a.repo),"show",f"{PIN}:{path}"]) for path in paths}

def extract(source, signature):
    start = source.index(signature)
    opening = source.index("{",start)
    depth = 0
    for end in range(opening,len(source)):
        depth += (source[end]=="{")-(source[end]=="}")
        if depth==0:return source[start:end+1]
    raise ValueError(signature)

extension_path="Source/ACE.Server/Network/Extensions.cs"
sources[extension_path]=subprocess.check_output(["git","-C",str(a.repo),"show",f"{PIN}:{extension_path}"])
ext=sources[extension_path].decode("utf-8-sig")
methods="\n".join(extract(ext,s) for s in ["private static uint CalculatePadMultiple(","public static void WriteString16L(","public static void Pad(this BinaryWriter"])
harness=r'''
using System;
using System.IO;
using System.Collections.Generic;
using ACE.Entity.Enum;
using ACE.Server.Network;
using ACE.Server.Network.GameAction.Actions;
using ACE.Server.Network.GameEvent;
using ACE.Server.Network.GameEvent.Events;
namespace ACE.Server.Entity { public class SalvageMessage { public uint Amount; public MaterialType MaterialType; public float Workmanship; public int NumItemsInMaterial; } }
namespace ACE.Server.Network {
 public enum GameMessageGroup { UIQueue }
 public class ClientMessage { public BinaryReader Payload; public ClientMessage(byte[] b){Payload=new(new MemoryStream(b));} }
 public class ConfirmationManager {public string Value;public void HandleResponse(ConfirmationType t,uint c,bool b){Value=$"{(uint)t},{c},{(b?1:0)}";}}
 public class Player {public int AugmentationBonusSalvage=4;public ConfirmationManager ConfirmationManager=new();public string Value;public void HandleSalvaging(uint tool,List<uint> items){Value=$"{tool},{string.Join(':',items)}";}}
 public class Session {public Player Player=new();}
 public static class Extensions { /*EXTENSIONS*/ }
}
namespace ACE.Server.Network.GameAction {public class GameActionAttribute:Attribute {public GameActionAttribute(GameActionType t){}}}
namespace ACE.Server.Network.GameEvent {
 public class GameEventMessage {
  protected BinaryWriter Writer; private MemoryStream stream=new();
  public GameEventMessage(GameEventType type,GameMessageGroup group,Session session,int capacity){Writer=new(stream);Writer.Write(0xF7B0u);Writer.Write(0x50000001u);Writer.Write(9u);Writer.Write((uint)type);}
  public byte[] Bytes()=>stream.ToArray();
 }
}
class Program {
 static void Emit(string name,GameEventMessage message)=>Console.WriteLine($"{name}\t{Convert.ToHexString(message.Bytes()).ToLowerInvariant()}");
 static void Main(){
  System.Text.Encoding.RegisterProvider(System.Text.CodePagesEncodingProvider.Instance);
  var session=new Session();
  Emit("confirm",new GameEventConfirmationRequest(session,ConfirmationType.CraftInteraction,17,"Apply salvage?"));
  Emit("confirm-alter-skill",new GameEventConfirmationRequest(session,ConfirmationType.AlterSkill,17,"Apply skill device?"));
  Emit("confirm-augmentation",new GameEventConfirmationRequest(session,ConfirmationType.Augmentation,17,"Apply skill device?"));
  Emit("done",new GameEventConfirmationDone(session,ConfirmationType.CraftInteraction,17));
  Emit("salvage",new GameEventSalvageOperationsResult(session,Skill.Salvaging,new(){new(){MaterialType=MaterialType.Iron,Workmanship=19f,NumItemsInMaterial=2,Amount=101}}));
  Emit("salvage-100",new GameEventSalvageOperationsResult(session,Skill.Salvaging,new(){new(){MaterialType=MaterialType.Iron,Workmanship=19f,NumItemsInMaterial=2,Amount=100}}));
  Emit("salvage-empty",new GameEventSalvageOperationsResult(session,Skill.Salvaging,new()));
  var salvage=Convert.FromHexString("02000000020000000300000004000000aa");
  var msg=new ClientMessage(salvage);GameActionCreateTinkeringTool.Handle(msg,session);
  Console.WriteLine($"input-salvage\t{Convert.ToHexString(salvage).ToLowerInvariant()}\t{session.Player.Value}\t{msg.Payload.BaseStream.Length-msg.Payload.BaseStream.Position}");
  foreach(uint response in new[]{0u,1u,uint.MaxValue}){
   using var stream=new MemoryStream();using var writer=new BinaryWriter(stream);writer.Write(5u);writer.Write(17u);writer.Write(response);writer.Write((byte)0xaa);
   var bytes=stream.ToArray();msg=new ClientMessage(bytes);GameActionConfirmationResponse.Handle(msg,session);
   Console.WriteLine($"input-confirm-{response}\t{Convert.ToHexString(bytes).ToLowerInvariant()}\t{session.Player.ConfirmationManager.Value}\t{msg.Payload.BaseStream.Length-msg.Payload.BaseStream.Position}");
  }
 }
}
'''.replace("/*EXTENSIONS*/",methods)
with tempfile.TemporaryDirectory(prefix="crafting-wire-oracle-") as tmp:
    tmp=Path(tmp)
    for i,(path,data) in enumerate(sources.items()):
        if path!=extension_path:(tmp/f"source{i}.cs").write_bytes(data)
    (tmp/"Program.cs").write_text(harness)
    (tmp/"Oracle.csproj").write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
    subprocess.run([a.dotnet,"build",str(tmp/"Oracle.csproj"),"--nologo","-v:q"],check=True)
    data=subprocess.check_output([a.dotnet,str(tmp/"bin/Debug/net8.0/Oracle.dll")]).decode()
header="# Official ACE contributors, AGPL-3.0-only; unchanged serializers/handlers.\n"
header+="# Synthetic session and event-envelope adapter; envelope parity independently covered by bace-compat.\n"
header+="".join(f"# {PIN} sha256 {hashlib.sha256(content).hexdigest()} {path}\n" for path,content in sources.items())
(root/"tests/fixtures/crafting.tsv").write_text(header+data)
