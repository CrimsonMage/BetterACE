#!/usr/bin/env python3
"""Run unchanged pinned ACE recall handlers; preserve ignored payload evidence."""
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
names = ["TeleToLifestone", "TeleToHouse", "TeleToMansion", "TeleToMarketplace", "TeleToPkArena", "TeleToPklArena", "RecallAllegianceHometown"]
base = "Source/ACE.Server/Network/GameAction/"
paths = [base + "GameActionType.cs"] + [base + "Actions/GameAction" + n + ".cs" for n in names]
sources = {path: subprocess.check_output(["git", "-C", str(a.repo), "show", f"{PIN}:{path}"]) for path in paths}
methods = [n.replace("Marketplace", "MarketPlace") for n in names]
player = "".join(f'public void HandleAction{n}(){{Value="{n}";}}' for n in methods)
harness = '''
using System;
using System.IO;
using System.Linq;
using ACE.Server.Network;
using ACE.Server.Network.GameAction;
namespace ACE.Server.Network {
 public class ClientMessage {public BinaryReader Payload;public ClientMessage(byte[] b){Payload=new(new MemoryStream(b));}}
 public class Player {public string Value;/*PLAYER*/}
 public class Session {public Player Player=new();}
}
namespace ACE.Server.Network.GameAction {
 [AttributeUsage(AttributeTargets.Method)] public class GameActionAttribute:Attribute {public GameActionType Type;public GameActionAttribute(GameActionType t){Type=t;}}
}
class Program {
 static void Main(){
  foreach(var type in typeof(Program).Assembly.GetTypes().OrderBy(t=>t.Name)){
   var method=type.GetMethod("Handle");if(method==null)continue;
   var attribute=(GameActionAttribute)Attribute.GetCustomAttribute(method,typeof(GameActionAttribute));if(attribute==null)continue;
   foreach(var bytes in new[]{Array.Empty<byte>(),new byte[]{0xde,0xad,0xbe,0xef}}){
    var session=new Session();var message=new ClientMessage(bytes);method.Invoke(null,new object[]{message,session});
    Console.WriteLine($"{(uint)attribute.Type}\\t{Convert.ToHexString(bytes)}\\t{session.Player.Value}\\t{message.Payload.BaseStream.Length-message.Payload.BaseStream.Position}");
   }
  }
 }
}
'''.replace("/*PLAYER*/", player)
with tempfile.TemporaryDirectory(prefix="bace-recall-oracle-") as temp:
    temp = Path(temp)
    for i, data in enumerate(sources.values()):
        (temp / f"source{i}.cs").write_bytes(data)
    (temp / "Program.cs").write_text(harness)
    (temp / "Oracle.csproj").write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
    subprocess.run([a.dotnet, "build", str(temp / "Oracle.csproj"), "--nologo", "-v:q"], check=True)
    output = subprocess.check_output([a.dotnet, str(temp / "bin/Debug/net8.0/Oracle.dll")]).decode()
header = "# Original ACE contributors, AGPL-3.0-only; unchanged action handlers and enum.\n"
header += "# Synthetic Player records invoked method; BinaryReader records ignored payload; no gameplay implementation.\n"
header += "".join(f"# {PIN} sha256 {hashlib.sha256(data).hexdigest()} {path}\n" for path, data in sources.items())
(root / "tests/fixtures/recall.tsv").write_text(header + output)
