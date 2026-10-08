#!/usr/bin/env python3
"""Compile original ACE AdvocateTeleport handler/reader/position constructor.
Synthetic geometry supplies explicit terrain and building data, never DAT assets.
"""
from pathlib import Path
import tempfile,subprocess,json,hashlib
ROOT=Path(__file__).resolve().parents[4]
SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
HANDLER=SRC/'ACE.Server/Network/GameAction/Actions/GameActionAdvocateTeleport.cs'
source=(SRC/'ACE.Entity/Position.cs').read_text();start=source.index('        public Position(BinaryReader payload)');end=source.index('        public Position(float northSouth',start);constructor=source[start:end]
source=(SRC/'ACE.Server/Entity/PositionExtensions.cs').read_text();start=source.index('        public static void AdjustMapCoords(');end=source.index('        public static void Translate(',start);adjust=source[start:end]
HARNESS=r'''
using System;using System.IO;using System.Text;using System.Text.Json;using ACE.Server.Network;using ACE.Server.Network.GameAction.Actions;
class Program {
 static void Main(){var rows=new List<object>();for(int flags=0;flags<8;flags++)foreach(bool water in new[]{false,true})foreach(bool building in new[]{false,true}){
  using var stream=new MemoryStream();using(var w=new BinaryWriter(stream,Encoding.UTF8,true)){w.Write((ushort)3);w.Write(Encoding.UTF8.GetBytes("map"));w.Write(new byte[3]);w.Write(0x12340001u);foreach(float x in new[]{10f,20f,9999f,1f,0f,0f,0f})w.Write(x);}var bytes=stream.ToArray();
  var session=new Session();session.Player.IsAdmin=(flags&1)!=0;session.Player.IsArch=(flags&2)!=0;session.Player.IsPsr=(flags&4)!=0;ACE.Server.Physics.Common.LScape.Water=water;ACE.Server.Physics.Common.LScape.Building=building;
  var message=new ClientMessage{Payload=new BinaryReader(new MemoryStream(bytes))};GameActionAdvocateTeleport.Handle(message,session);
  rows.Add(new {flags,water,building,bytes=Convert.ToHexString(bytes),consumed=message.Payload.BaseStream.Position,teleported=session.Player.Target!=null,cell=session.Player.Target?.Cell,z=session.Player.Target?.PositionZ});
 }Console.WriteLine(JsonSerializer.Serialize(rows));}
}
namespace ACE.Entity.Enum {public enum ChatMessageType {Broadcast}}
namespace ACE.Entity {public class LandblockId{public uint Raw;public uint Landblock=>Raw>>16;public LandblockId(uint value){Raw=value;}}public class Position{
 public LandblockId LandblockId;public uint Cell=>LandblockId.Raw;public float PositionX,PositionY,PositionZ,RotationW,RotationX,RotationY,RotationZ;
 CONSTRUCTOR
 public string GetMapCoordStr()=>"map";public float GetTerrainZ()=>42f;public uint GetCell()=>0x12340101u;
}}
namespace ACE.Server.Entity {public static class PositionExtensions {ADJUST}}
namespace ACE.Server.Physics.Common {
 public static class LandDefs{public enum WaterType{None,EntirelyWater}}
 public class Block{public LandDefs.WaterType WaterType;}
 public class Building{public float GetMinZ()=>4f;}
 public class SortCell{public Building Building=new();public bool has_building()=>LScape.Building;}
 public static class LScape{public static bool Water,Building;public static Block get_landblock(uint x)=>new(){WaterType=Water?LandDefs.WaterType.EntirelyWater:LandDefs.WaterType.None};public static object get_landcell(uint x)=>new SortCell();}
}
namespace ACE.Server.Network {public enum GameActionType{AdvocateTeleport}public class GameActionAttribute:Attribute{public GameActionAttribute(GameActionType x){}}public class ClientMessage{public BinaryReader Payload;}public class Session{public Player Player=new();}public class Player{public bool IsAdmin,IsArch,IsPsr;public ACE.Entity.Position Target;public void Teleport(ACE.Entity.Position p){Target=p;}}public static class ChatPacket{public static void SendServerMessage(Session s,string m,ACE.Entity.Enum.ChatMessageType t){}}}
'''.replace('CONSTRUCTOR',constructor).replace('ADJUST',adjust)
HARNESS='using ACE.Entity;using ACE.Server.Physics.Common;\n'+HARNESS
with tempfile.TemporaryDirectory(prefix='staff-map-oracle-') as tmp:
 p=Path(tmp);(p/'Program.cs').write_text(HARNESS);(p/'Handler.cs').write_bytes(HANDLER.read_bytes());(p/'Readers.cs').write_bytes((SRC/'ACE.Common/Extensions/BinaryReaderExtensions.cs').read_bytes())
 (p/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 subprocess.run(['/tmp/bace-crafting-dotnet/dotnet','build','--nologo','-o',str(p/'build')],cwd=p,check=True,stdout=subprocess.DEVNULL)
 raw=subprocess.check_output(['/tmp/bace-crafting-dotnet/dotnet',str(p/'build/oracle.dll')],text=True)
 target=Path(__file__).resolve().parents[1]/'tests/fixtures/staff_map.csv'
 lines=['# Original ACE handler '+hashlib.sha256(HANDLER.read_bytes()).hexdigest(),'flags,water,building,bytes,consumed,teleported,cell,z']
 for r in json.loads(raw):lines.append(','.join(str(r[k]).lower() if r[k] is not None else '-' for k in ['flags','water','building','bytes','consumed','teleported','cell','z']))
 target.write_text('\n'.join(lines)+'\n');print(target)
