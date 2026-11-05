#!/usr/bin/env python3
"""Compile pinned ACE housing GameEvent constructors and emit tests/fixtures/housing_output.csv.

Verbatim from the pinned ACE snapshot (compiled unchanged):
  - GameEvent/GameEventType.cs and the nine GameEvent/Events/GameEventHouse*.cs classes
  - Structure/GuestInfo.cs, PackableList.cs, PackableHashTable.cs, Extensions.cs (string16)
  - Entity enums HouseType, HouseBitfield, HARBitfield, WeenieError; Sequence/SequenceType.cs, ByteSequence.cs
Extracted by brace matching from larger files (only the named writer members are compiled):
  - HashComparer.cs: GuidComparer
  - HouseProfile.cs, HouseData.cs, HousePayment.cs, HouseAccess.cs, RestrictionDB.cs: *Extensions writers
  - AllegianceHierarchy.cs: Write(Position), Write(Vector3), Write(Quaternion)
Synthetic stubs (gameplay state, not wire layout): ObjectGuid, Position, Session, GameEventMessage
header, WorldObject, House, SequenceManager, Player, and the data classes whose constructors touch
the database or world. The GameEvent constructors and all writers listed above are verbatim.
ACE's HouseAccess(House) population logic is replaced by a stub that copies a prepared record.
Every compiled source is hashed in the fixture header.
"""
import hashlib
import os
import re
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
SRC = ROOT / '.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
NETWORK = SRC / 'ACE.Server/Network'
DOTNET = os.environ.get('BACE_DOTNET', '/home/jokerfactor/.dotnet/dotnet')
TARGET = Path(__file__).resolve().parents[1] / 'tests/fixtures/housing_output.csv'

WHOLE = [
    NETWORK / 'GameEvent/GameEventType.cs',
    NETWORK / 'GameEvent/Events/GameEventHouseAvailableHouses.cs',
    NETWORK / 'GameEvent/Events/GameEventHouseData.cs',
    NETWORK / 'GameEvent/Events/GameEventHouseProfile.cs',
    NETWORK / 'GameEvent/Events/GameEventHouseStatus.cs',
    NETWORK / 'GameEvent/Events/GameEventHouseTransaction.cs',
    NETWORK / 'GameEvent/Events/GameEventHouseUpdateHAR.cs',
    NETWORK / 'GameEvent/Events/GameEventHouseUpdateRentPayment.cs',
    NETWORK / 'GameEvent/Events/GameEventHouseUpdateRentTime.cs',
    NETWORK / 'GameEvent/Events/GameEventHouseUpdateRestrictions.cs',
    NETWORK / 'Structure/GuestInfo.cs',
    NETWORK / 'Structure/PackableList.cs',
    NETWORK / 'Structure/PackableHashTable.cs',
    NETWORK / 'Extensions.cs',
    NETWORK / 'Sequence/SequenceType.cs',
    NETWORK / 'Sequence/ISequence.cs',
    NETWORK / 'Sequence/ByteSequence.cs',
    SRC / 'ACE.Entity/Enum/HouseType.cs',
    SRC / 'ACE.Entity/Enum/HouseBitfield.cs',
    SRC / 'ACE.Entity/Enum/HARBitfield.cs',
    SRC / 'ACE.Entity/Enum/WeenieError.cs',
]

# (source file, member header to extract by brace matching)
EXTRACT = [
    (NETWORK / 'Structure/HashComparer.cs', 'public class GuidComparer'),
    (NETWORK / 'Structure/HouseProfile.cs', 'public static class HouseProfileExtensions'),
    (NETWORK / 'Structure/HouseData.cs', 'public static class HouseDataExtensions'),
    (NETWORK / 'Structure/HousePayment.cs', 'public static class HousePaymentExtensions'),
    (NETWORK / 'Structure/HouseAccess.cs', 'public static class HouseAccessExtensions'),
    (NETWORK / 'Structure/RestrictionDB.cs', 'public static class RestrictionDBExtensions'),
    (NETWORK / 'Structure/AllegianceHierarchy.cs', 'public static void Write(this BinaryWriter writer, Position position)'),
    (NETWORK / 'Structure/AllegianceHierarchy.cs', 'public static void Write(this BinaryWriter writer, Vector3 v)'),
    (NETWORK / 'Structure/AllegianceHierarchy.cs', 'public static void Write(this BinaryWriter writer, Quaternion q)'),
]

HEADER = '''using System;
using System.IO;
using System.Text;
using System.Linq;
using System.Numerics;
using System.Collections.Generic;
using ACE.Entity;
using ACE.Entity.Enum;
using ACE.Server.Network;
using ACE.Server.Network.Enum;
using ACE.Server.Network.GameEvent;
using ACE.Server.Network.GameEvent.Events;
using ACE.Server.Network.Sequence;
using ACE.Server.Network.Structure;
using ACE.Server.WorldObjects;
using ACE.Database.Models.Shard;
namespace ACE.Server.Network.Enum { public enum StubEnum { None } }
namespace ACE.Entity {
 public struct ObjectGuid { public uint Full; public ObjectGuid(uint id) { Full = id; } public static readonly ObjectGuid Invalid = new ObjectGuid(0); }
 public class Position { public uint Cell; public Vector3 Pos; public Quaternion Rotation; }
}
namespace ACE.Database.Models.Shard { public class CharacterPropertiesFillCompBook { public uint SpellComponentId, QuantityToRebuy; } }
namespace ACE.Server.Network {
 public enum GameMessageGroup { UIQueue }
 public class Player { public ObjectGuid Guid; }
 public class Session { public Player Player = new Player { Guid = new ObjectGuid(0x50000001) }; public uint GameEventSequence = 0x2A; }
}
namespace ACE.Server.Network.GameEvent {
 public abstract class GameEventMessage {
  public MemoryStream Data = new MemoryStream();
  public BinaryWriter Writer;
  protected GameEventMessage(GameEventType op, GameMessageGroup group, Session session, int capacity = 0) {
   Writer = new BinaryWriter(Data);
   Writer.Write(0xf7b0u);
   Writer.Write(session.Player.Guid.Full);
   Writer.Write(session.GameEventSequence++);
   Writer.Write((uint)op);
  }
 }
}
namespace ACE.Server.Network.Sequence {
 public class SequenceManager {
  private readonly ByteSequence sequence = new ByteSequence(0x10);
  public byte[] GetNextSequence(SequenceType type) => sequence.NextBytes;
 }
}
namespace ACE.Server.WorldObjects {
 public class WorldObject { public SequenceManager Sequences; public ObjectGuid Guid; }
 public class House { public ACE.Server.Network.Structure.HouseAccess Prepared; }
}
namespace ACE.Server.Network.Structure {
 public class HousePayment { public int Num; public int Paid; public uint WeenieID; public string Name; public string PluralName; }
 public class HouseProfile {
  public uint DwellingID; public ObjectGuid OwnerID; public HouseBitfield Bitmask;
  public int MinLevel; public int MaxLevel; public int MinAllegRank; public int MaxAllegRank;
  public bool MaintenanceFree; public HouseType Type; public string OwnerName;
  public List<HousePayment> Buy; public List<HousePayment> Rent;
 }
 public class HouseData {
  public uint BuyTime; public uint RentTime; public HouseType Type; public bool MaintenanceFree;
  public List<HousePayment> Buy; public List<HousePayment> Rent; public Position Position;
 }
 public class HouseAccess {
  public uint Version = 0x10000002; public HARBitfield Bitmask; public ObjectGuid MonarchID;
  public Dictionary<ObjectGuid, GuestInfo> GuestList = new Dictionary<ObjectGuid, GuestInfo>();
  public List<ObjectGuid> Roommates = new List<ObjectGuid>();
  public HouseAccess() { }
  public HouseAccess(ACE.Server.WorldObjects.House house) {
   var p = house.Prepared;
   Version = p.Version; Bitmask = p.Bitmask; MonarchID = p.MonarchID;
   GuestList = p.GuestList; Roommates = p.Roommates;
  }
 }
 public class RestrictionDB {
  public uint HouseOwner; public uint Version = 0x10000002; public bool OpenStatus;
  public ObjectGuid MonarchID; public Dictionary<ObjectGuid, uint> Table = new Dictionary<ObjectGuid, uint>();
 }
}
'''

SCENARIOS = '''
namespace Harness {
 public static class Program {
  static HousePayment P(int num, int paid, uint wcid, string name, string plural) =>
   new HousePayment { Num = num, Paid = paid, WeenieID = wcid, Name = name, PluralName = plural };
  static void Emit(string name, GameEventMessage message) =>
   Console.WriteLine("output," + name + "," + Convert.ToHexString(message.Data.ToArray()));
  static void Main() {
   Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
   var s = new Session();
   var buy = new List<HousePayment> { P(2, 1, 273, "Pyreal", "Pyreals"), P(1, 0, 0x9000, "Key", "Keys") };
   var rent = new List<HousePayment> { P(5, 5, 273, "Pyreal", "Pyreals") };
   Emit("profile", new GameEventHouseProfile(s, new ObjectGuid(0x70000001), new HouseProfile {
    DwellingID = 0x1234, OwnerID = new ObjectGuid(0x50000002), Bitmask = HouseBitfield.Active,
    MinLevel = -1, MaxLevel = 50, MinAllegRank = -1, MaxAllegRank = 3, MaintenanceFree = true,
    Type = HouseType.Villa, OwnerName = "Caf\\u00e9", Buy = buy, Rent = rent }));
   Emit("profile_empty", new GameEventHouseProfile(s, new ObjectGuid(0x70000002), new HouseProfile {
    DwellingID = 0, OwnerID = new ObjectGuid(0), Bitmask = HouseBitfield.RequiresMonarch,
    MinLevel = -1, MaxLevel = -1, MinAllegRank = -1, MaxAllegRank = -1, MaintenanceFree = false,
    Type = HouseType.Undef, OwnerName = "", Buy = new List<HousePayment>(), Rent = new List<HousePayment>() }));
   Emit("data", new GameEventHouseData(s, new HouseData {
    BuyTime = 1000, RentTime = 2000, Type = HouseType.Cottage, MaintenanceFree = false,
    Buy = new List<HousePayment> { P(1, 0, 1, "Coin", "Coins") }, Rent = new List<HousePayment>(),
    Position = new Position { Cell = 0x12340001, Pos = new Vector3(1.5f, -2.25f, 3f), Rotation = new Quaternion(0.5f, 0.25f, 0f, 1f) } }));
   var prepared = new HouseAccess {
    Bitmask = HARBitfield.OpenHouse | HARBitfield.AllegianceGuests, MonarchID = new ObjectGuid(0x50000009),
    Roommates = new List<ObjectGuid> { new ObjectGuid(0x50000003), new ObjectGuid(0x50000004) } };
   prepared.GuestList[new ObjectGuid(0x50000001)] = new GuestInfo(false, "Alpha");
   prepared.GuestList[new ObjectGuid(0x50000041)] = new GuestInfo(true, "Bravo");
   prepared.GuestList[new ObjectGuid(0x50000002)] = new GuestInfo(false, "Charlie");
   Emit("access", new GameEventUpdateHAR(s, new ACE.Server.WorldObjects.House { Prepared = prepared }));
   Emit("access_empty", new GameEventUpdateHAR(s, new ACE.Server.WorldObjects.House { Prepared = new HouseAccess() }));
   Emit("status_default", new GameEventHouseStatus(s));
   Emit("status_nomem", new GameEventHouseStatus(s, WeenieError.NoMem));
   Emit("rent_time", new GameEventHouseUpdateRentTime(s));
   Emit("rent_payments", new GameEventHouseUpdateRentPayment(s));
   Emit("transaction", new GameEventHouseTransaction(s));
   var obj = new ACE.Server.WorldObjects.WorldObject { Sequences = new SequenceManager(), Guid = new ObjectGuid(0x70000003) };
   var db = new RestrictionDB { OpenStatus = false, MonarchID = new ObjectGuid(0x50000009) };
   db.Table[new ObjectGuid(0x50000001)] = 0;
   db.Table[new ObjectGuid(0x50000041)] = 1;
   db.Table[new ObjectGuid(0x50000002)] = 1;
   db.Table[new ObjectGuid(0x50000007)] = 0;
   Emit("restrictions", new GameEventHouseUpdateRestrictions(s, obj, db));
   Emit("available", new GameEventHouseAvailableHouses(s, HouseType.Mansion, new List<uint> { 0x70000001, 0x70000002 }, 9));
  }
 }
}
'''


def strip_usings(text):
    """Using directives are hoisted into HEADER; they cannot sit between namespace blocks."""
    return '\n'.join(line for line in text.splitlines() if not line.startswith('using '))


def block(text, header):
    """Return the member starting at `header` through its matching closing brace."""
    start = text.index(header)
    depth = 0
    for index in range(text.index('{', start), len(text)):
        if text[index] == '{':
            depth += 1
        elif text[index] == '}':
            depth -= 1
            if depth == 0:
                return text[start:index + 1]
    raise ValueError(f'unbalanced braces for {header!r}')


def main():
    sources = list(WHOLE)
    chunks = []
    for path in WHOLE:
        chunks.append(strip_usings(path.read_text(encoding='utf-8')))
    extracted = {}
    for path, header in EXTRACT:
        if path not in extracted:
            extracted[path] = []
            if path not in sources:
                sources.append(path)
        extracted[path].append(block(strip_usings(path.read_text(encoding='utf-8')), header))
    # Position writers share one static class in the harness namespace.
    position_writers = 'namespace ACE.Server.Network.Structure { public static class PositionWriters {\n' + \
        '\n'.join(extracted[NETWORK / 'Structure/AllegianceHierarchy.cs']) + '\n} }\n'
    for path, members in extracted.items():
        if path.name == 'AllegianceHierarchy.cs':
            continue
        chunks.append('namespace ACE.Server.Network.Structure {\n' + '\n'.join(members) + '\n}\n')
    chunks.append(position_writers)

    program = HEADER + '\n'.join(chunks) + SCENARIOS
    with tempfile.TemporaryDirectory(prefix='housing-output-oracle-') as tmp:
        p = Path(tmp)
        (p / 'Program.cs').write_text(program, encoding='utf-8')
        (p / 'oracle.csproj').write_text(
            '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType>'
            '<TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings>'
            '<Nullable>disable</Nullable><TreatWarningsAsErrors>false</TreatWarningsAsErrors>'
            '</PropertyGroup></Project>',
            encoding='utf-8',
        )
        subprocess.run([DOTNET, 'build', '--nologo', '-o', str(p / 'build')], cwd=p, check=True)
        output = subprocess.check_output([DOTNET, str(p / 'build/oracle.dll')], text=True)

    header_lines = []
    for path in sources:
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        header_lines.append(f'# {path.relative_to(SRC)} sha256={digest}')
    body = '\n'.join(header_lines) + '\n' + output
    TARGET.write_text(body, encoding='utf-8', newline='\n')
    print(TARGET)


if __name__ == '__main__':
    main()
