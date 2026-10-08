"""Unchanged official ACE missile handler against synthetic BinaryReader input."""
import argparse,hashlib,subprocess,tempfile,urllib.request
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args();pin='47edade3bd3f6044b676d4eb877c4965c7eda62b'
rel='Source/ACE.Server/Network/GameAction/Actions/GameActionTargetedMissileAttack.cs';data=(a.source/rel).read_bytes();assert data==urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{pin}/{rel}',timeout=30).read()
h='''using System;using System.IO;
namespace ACE.Server.Network.GameAction.Actions {
 class GameActionAttribute:Attribute{public GameActionAttribute(GameActionType t){}}
 enum GameActionType{TargetedMissileAttack}
 public class ClientMessage{public BinaryReader Payload;public ClientMessage(byte[] b){Payload=new(new MemoryStream(b));}}
 public class Session{public Player Player=new();}
 public class Player{public uint Target,Height;public float Accuracy;public void HandleActionTargetedMissileAttack(uint t,uint h,float a){Target=t;Height=h;Accuracy=a;}}
}
class Program{static void Main(){foreach(uint target in new uint[]{1,0x50000001,0xffffffff})foreach(uint height in new uint[]{1,2,3,0xffffffff})foreach(uint bits in new uint[]{0,0x3f000000,0x3f800000,0x7fc00001}){using var m=new MemoryStream();using(var w=new BinaryWriter(m,System.Text.Encoding.UTF8,true)){w.Write(target);w.Write(height);w.Write(bits);}var b=m.ToArray();var session=new ACE.Server.Network.GameAction.Actions.Session();ACE.Server.Network.GameAction.Actions.GameActionTargetedMissileAttack.Handle(new(b),session);Console.WriteLine($"{Convert.ToHexString(b)},{session.Player.Target},{session.Player.Height},{BitConverter.SingleToUInt32Bits(session.Player.Accuracy)}");}}}
'''+data.decode('utf-8-sig')
with tempfile.TemporaryDirectory(prefix='bace-missile-wire-')as t:
 b=Path(t);(b/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><RollForward>Major</RollForward></PropertyGroup></Project>');(b/'Program.cs').write_text(h);subprocess.run([a.dotnet,'build','--nologo','-o',str(b/'out'),str(b/'Oracle.csproj')],check=True);out=subprocess.check_output([a.dotnet,str(b/'out/Oracle.dll')],text=True)
Path('crates/network/bace-wire/tests/fixtures/missile.csv').write_text(f'# official ACE {pin}; SHA256 {hashlib.sha256(data).hexdigest()} {rel}\n'+out)
