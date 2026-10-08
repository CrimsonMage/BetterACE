#!/usr/bin/env python3
"""Compile pinned ACE NoCorpse dispatch and unchanged GenerateTreasure.

The CreateCorpse prefix ends at the first normal-corpse statement. Reaching that
boundary is recorded, not emulated. GenerateTreasure's factory outputs, selected
create-list indices, property storage, inventory/dequip, logging and landblock
admission are explicit stubs. This proves dispatch/filter/order/quest-ID/position
copy behavior, not treasure probabilities, player loss selection, collision,
container construction, networking or durable admission. Player CalculateDeathItems
is textually after the exercised early return and is never called by this prefix.
No proprietary assets are used. Original source retains ACEmulator attribution.
"""

from pathlib import Path
import hashlib
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[4]
PIN = "47edade3bd3f6044b676d4eb877c4965c7eda62b"
SOURCE = ROOT / ".reference" / ("ACE-" + PIN) / "Source"
PATH = SOURCE / "ACE.Server/WorldObjects/Creature_Death.cs"
text = PATH.read_text()
start = text.index("            if (NoCorpse)", text.index("protected void CreateCorpse"))
end = text.index("            var cachedWeenie", start)
prefix = text[start:end]
assert text.index("player.CalculateDeathItems(", end) > end
start = text.index("        private List<WorldObject> GenerateTreasure(")
end = text.index("\n        /// <summary>", start)
treasure = text[start:end]
enum_paths = [SOURCE / ("ACE.Entity/Enum/" + name + ".cs")
              for name in ("DestinationType", "BondedStatus")]

program = r'''using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using ACE.Entity.Enum;
class ObjectGuid { public uint Full; public ObjectGuid(uint id){Full=id;}
    public static readonly ObjectGuid Invalid=new(0); }
class Position { public uint Cell; public float[] Values;
    public Position(uint c,float[] values){Cell=c;Values=values;}
    public Position(Position p){Cell=p.Cell;Values=(float[])p.Values.Clone();} }
class WorldObject { public ObjectGuid Guid; public string Quest; public uint? GeneratorId=777;
    public Position Location; public DestinationType DestinationType; public BondedStatus Bonded;
    public WorldObject(uint id,DestinationType dest,string quest=null){Guid=new(id);DestinationType=dest;Quest=quest;} }
class DamageHistoryInfo { public bool IsOlthoiPlayer; }
class CreateRow { public uint Id; public DestinationType DestinationType; }
class Biota { public List<CreateRow> PropertiesCreateList; }
class Corpse { public ObjectGuid Guid=new(900); public void TryAddToInventory(WorldObject item){throw new Exception("unexpected corpse path");} }
enum PropertyInstanceId { Wielder,Container }
class GameMessagePublicUpdateInstanceID {
    public GameMessagePublicUpdateInstanceID(WorldObject item,PropertyInstanceId property,ObjectGuid value){Trace.Events.Add($"iid:{item.Guid.Full}:{property}:{value.Full}");} }
class GameMessagePickupEvent { public GameMessagePickupEvent(WorldObject item){throw new Exception("unexpected corpse pickup");} }
static class Trace { public static List<string> Events=new(); public static List<WorldObject> Added=new(); }
static class PropertyManager { public static bool DropWield; public static (bool Item,bool Other) GetBool(string name){if(name!="creatures_drop_createlist_wield")throw new Exception(name);return (DropWield,false);} }
static class LootGenerationFactory { public static List<WorldObject> CreateRandomLootObjects(object profile){Trace.Events.Add("death-factory");return new(){new(10,0),new(11,0," ")};} }
static class WorldObjectFactory { public static WorldObject CreateNewWorldObject(CreateRow row){Trace.Events.Add("create:"+row.Id);return row.Id==35?null:new(row.Id,row.DestinationType,row.Id==34?"quest":null);} }
static class LandblockManager { public static void AddObject(WorldObject item){Trace.Events.Add("world:"+item.Guid.Full);Trace.Added.Add(item);} }
class Creature {
    public bool NoCorpse; public bool NormalCorpseBoundary; public int Scenario;
    public ObjectGuid Guid=new(0x50000042); public Position Location;
    public object DeathTreasure; public Biota Biota=new();
    public Dictionary<uint,WorldObject> Inventory=new(),EquippedObjects=new();
    public void Run(DamageHistoryInfo killer) {
PREFIX
        NormalCorpseBoundary=true;
    }
    private void DoCantripLogging(DamageHistoryInfo killer,WorldObject item){Trace.Events.Add("cantrip-log:"+item.Guid.Full);}
    private bool TryDequipObjectWithBroadcasting(ObjectGuid guid,out WorldObject item,out int location){location=0;var result=EquippedObjects.Remove(guid.Full,out item);if(result)Trace.Events.Add("dequip:"+guid.Full);return result;}
    private void EnqueueBroadcast(params object[] messages){Trace.Events.Add("broadcast:"+messages.Length);}
    private List<CreateRow> CreateListSelect(List<CreateRow> rows){Trace.Events.Add("select:"+string.Join(",",rows.Select(r=>r.Id)));return rows.Where(r=>Scenario!=2||r.Id%2==0).ToList();}
TREASURE
}
class Program {
    static string Dump(WorldObject item,Position location)=>$"{item.Guid.Full}:{item.GeneratorId}:{item.Location.Cell:X8}:"+string.Join(",",item.Location.Values.Select(v=>BitConverter.SingleToInt32Bits(v).ToString("X8")))+":"+(!ReferenceEquals(item.Location,location)&&!ReferenceEquals(item.Location.Values,location.Values));
    static void Main(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
        for(int noCorpse=0;noCorpse<2;noCorpse++)for(int killer=0;killer<3;killer++)
        for(int dropWield=0;dropWield<2;dropWield++)for(int scenario=0;scenario<3;scenario++)for(int pose=0;pose<3;pose++){
            Trace.Events.Clear();Trace.Added.Clear();PropertyManager.DropWield=dropWield!=0;
            var location=new Position(pose==2?0x12340100u:0x12340001u,new float[]{12.5f,-3.25f,pose==0?0f:pose==1?-0.125f:123.75f,0.5f,-0.5f,0.5f,-0.5f});
            var c=new Creature{NoCorpse=noCorpse!=0,Scenario=scenario,Location=location,DeathTreasure=scenario==0?null:new object()};
            c.Inventory.Add(20,new(20,0));c.Inventory.Add(22,new(22,DestinationType.Treasure,"quest"));
            c.Inventory.Add(24,new(24,DestinationType.Contain));
            c.EquippedObjects.Add(21,new(21,DestinationType.Wield,""));
            c.EquippedObjects.Add(23,new(23,DestinationType.WieldTreasure){Bonded=BondedStatus.Destroy});
            if(scenario!=0)c.Biota.PropertiesCreateList=new(){new(){Id=30,DestinationType=DestinationType.Contain},new(){Id=31,DestinationType=DestinationType.Treasure},new(){Id=32,DestinationType=DestinationType.Wield},new(){Id=33,DestinationType=DestinationType.WieldTreasure},new(){Id=34,DestinationType=DestinationType.ContainTreasure},new(){Id=35,DestinationType=DestinationType.Contain}};
            c.Run(killer==0?null:new(){IsOlthoiPlayer=killer==2});
            Console.WriteLine($"{noCorpse}|{killer}|{dropWield}|{scenario}|{pose}|{c.NormalCorpseBoundary}|{string.Join(";",Trace.Events)}|{string.Join(";",Trace.Added.Select(i=>Dump(i,location)))}|{string.Join(",",c.Inventory.Keys)}|{string.Join(",",c.EquippedObjects.Keys)}");
        }
    }
}
'''.replace("PREFIX", prefix).replace("TREASURE", treasure)

with tempfile.TemporaryDirectory(prefix="bace-no-corpse-") as tmp:
    directory = Path(tmp)
    (directory / "Program.cs").write_text(program)
    for path in enum_paths:
        (directory / path.name).write_bytes(path.read_bytes())
    (directory / "oracle.csproj").write_text(
        '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType>'
        '<TargetFramework>net8.0</TargetFramework><WarningLevel>0</WarningLevel>'
        '</PropertyGroup></Project>')
    dotnet = os.environ.get("BACE_DOTNET", "dotnet")
    subprocess.run([dotnet, "build", "--nologo", "-o", str(directory / "build")],
                   cwd=directory, check=True)
    output = subprocess.check_output([dotnet, str(directory / "build/oracle.dll")], text=True)
    assert len(output.splitlines()) == 108
    header = "# Official ACE " + PIN + "\n"
    header += "# Original NoCorpse prefix and complete GenerateTreasure; helper stubs documented in oracle/no_corpse.py.\n"
    header += "# no_corpse|killer(0=null,1=ordinary,2=olthoi)|drop_wield|scenario|pose|normal_corpse_boundary|ordered_calls|world_roots(id:generator:cell:posebits:copied)|remaining_inventory|remaining_equipment\n"
    header += "".join("# " + str(path.relative_to(SOURCE)) + " sha256="
                      + hashlib.sha256(path.read_bytes()).hexdigest() + "\n"
                      for path in [PATH, *enum_paths])
    fixture = Path(__file__).resolve().parents[1] / "tests/fixtures/no_corpse.trace"
    fixture.write_text(header + output)
    print("108 original NoCorpse dispatch/treasure/placement vectors")
