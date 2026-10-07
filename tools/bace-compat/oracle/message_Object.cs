using System;
using System.IO;
using System.Linq;
using System.Collections.Generic;
using System.Numerics;
using ACE.Entity;
using ACE.Entity.Enum;
using ACE.Entity.Models;
using ACE.Server.Entity;
using ACE.Server.Network;
using ACE.Server.Network.Structure;
using ACE.Server.Network.GameMessages;
using ACE.Server.Network.GameMessages.Messages;
using ACE.Server.WorldObjects;

static class ObjectHarness {
    static object Message(GameMessage message) => new { bytes=Convert.ToHexString(message.Data.ToArray()),group=(uint)message.Group };
    static ObjDesc Model(int mode) {
        var model=new ObjDesc{PaletteID=0x04008000};
        if ((mode&1)!=0) model.SubPalettes.AddRange(new[] {new PropertiesPalette{SubPaletteId=0x04007fff,Offset=0,Length=255},new PropertiesPalette{SubPaletteId=0x04008000,Offset=255,Length=0}});
        if ((mode&2)!=0) model.TextureChanges.AddRange(new[] {new PropertiesTextureMap{PartIndex=7,OldTexture=0x05007fff,NewTexture=0x05008000},new PropertiesTextureMap{PartIndex=255,OldTexture=0x05000001,NewTexture=0x0500ffff}});
        if ((mode&4)!=0) model.AnimPartChanges.AddRange(new[] {new PropertiesAnimPart{Index=3,AnimationId=0x01007fff},new PropertiesAnimPart{Index=255,AnimationId=0x01008000}});
        return model;
    }
    public static House Input() {
        var value=new House{Guid=new ObjectGuid(0x50000001),Name="Café object",WeenieClassId=0x123456,IconId=0x06008000,ItemType=0x12345678,ObjectDescriptionFlags=(ObjectDescriptionFlag)0x10,
            Model=Model(7),PhysicsHeader=(PhysicsDescriptionFlag)0x5fbff,HarnessState=(PhysicsState)0x01020304,
            PluralName="Café objects",ItemCapacity=7,ContainerCapacity=8,AmmoType=0x1122,Value=123456,ItemUseable=0x12345678,UseRadius=1.25f,
            TargetType=8,UiEffects=0xf00,CombatUse=-2,Structure=0x1234,MaxStructure=0x2345,StackSize=0x3456,MaxStackSize=0x4567,
            ContainerId=0x80000001,WielderId=0x50000002,ValidLocations=0x12345678,CurrentWieldedLocation=0x23456789,ClothingPriority=0x34567890,
            RadarColor=0xab,RadarBehavior=0xcd,Script=0xccdd,Workmanship=2.5f,EncumbranceVal=0x5678,SpellDID=0x6789,HouseOwner=0x50000003,
            HookItemType=0x45678901,MonarchId=0x50000004,HookType=0xabcd,IconOverlayId=0x06007fff,IconUnderlayId=0x06008000,MaterialType=0x56789012,
            CooldownId=-123,CooldownDuration=12.25,PetOwner=0x50000005,
            MotionTableId=0x09000001,SoundTableId=0x20000002,PhysicsTableId=0x34000003,SetupTableId=0x02000004,ParentLocation=(ParentLocation)19,
            ObjScale=1.25f,Friction=.5f,Elasticity=.75f,Translucency=.25f,Velocity=new Vector3(1,2,3),Acceleration=new Vector3(4,5,6),Omega=new Vector3(7,8,9),DefaultScriptId=0x33000001,DefaultScriptIntensity=1.5f,
            OpenStatus=true,HouseType=HouseType.Cottage,Placement=(Placement)8,
        };
        value.Children.Add(new HeldItem(0x80000001,19,0));value.Children.Add(new HeldItem(0x80000002,34,0));
        value.Guests.Add(new ObjectGuid(0x50000021),true);value.Guests.Add(new ObjectGuid(0x50000020+89),true);value.Guests.Add(new ObjectGuid(0x50000020),false);
        value.CurrentMotionState=new Motion{MovementType=MovementType.Invalid,IsAutonomous=true,MotionFlags=MotionFlags.StickToObject|MotionFlags.StandingLongJump,Stance=(MotionStance)61,TargetGuid=new ObjectGuid(0x80000001),
            MotionState=new InterpretedMotionState{ForwardCommand=MotionCommand.RunForward,ForwardSpeed=1.5f,Commands=new List<MotionItem>{new(value,MotionCommand.Wave,1.25f){IsAutonomous=true}}}};
        return value;
    }
    public static object Run() {
        var models=Enumerable.Range(0,8).Select(mode=>{var wo=Input();wo.Model=Model(mode);return new{mode,bytes=Convert.ToHexString(wo.ModelBytes()),model=wo.Model};}).ToList<object>();
        var large=Input();large.Model=Model(0);large.Model.SubPalettes=Enumerable.Range(0,255).Select(i=>new PropertiesPalette{SubPaletteId=0x04000000u+(uint)i,Offset=(ushort)i,Length=1}).ToList();
        models.Add(new{mode=255,bytes=Convert.ToHexString(large.ModelBytes()),model=large.Model});
        var physicsMasks=new uint[]{0,1,2,4,8,16,32,64,128,256,512,0x800,0x1000,0x2000,0x4000,0x8000,0x10000,0x20000,0x40000,0x5fbff,0x6fbff};
        var physics=physicsMasks.Select(mask=>{var wo=Input();wo.PhysicsHeader=(PhysicsDescriptionFlag)mask;return new{flags=mask,bytes=Convert.ToHexString(wo.PhysicsBytes())};}).ToArray();
        var headers=new List<(uint,uint)>{(0,0),(uint.MaxValue,15)};
        for(var i=0;i<32;i++)headers.Add((1u<<i,0));for(var i=0;i<4;i++)headers.Add((0,1u<<i));
        var game=headers.Select(pair=>{var wo=Input();wo.Header=(WeenieHeaderFlag)pair.Item1;wo.Header2=(WeenieHeaderFlag2)pair.Item2;if(pair.Item2!=0)wo.ObjectDescriptionFlags|=ObjectDescriptionFlag.IncludesSecondHeader;
            using var ms=new MemoryStream();using var writer=new BinaryWriter(ms);wo.SerializeGameDataOnly(writer);
            return new{flags=pair.Item1,flags2=pair.Item2,bytes=Convert.ToHexString(ms.ToArray())};}).ToArray();
        var full=Input();full.Header=(WeenieHeaderFlag)uint.MaxValue;full.Header2=(WeenieHeaderFlag2)15;full.ObjectDescriptionFlags|=ObjectDescriptionFlag.IncludesSecondHeader;
        var child=new WorldObject{Guid=new ObjectGuid(0x50000002),ParentLocation=(ParentLocation)19,Placement=(Placement)8};
        var messages=new Dictionary<string,object>{
            ["create"]=Message(new GameMessageCreateObject(full)),["update"]=Message(new GameMessageUpdateObject(full)),["appearance"]=Message(new GameMessageObjDescEvent(full)),
            ["delete"]=Message(new GameMessageDeleteObject(full)),["player_create"]=Message(new GameMessagePlayerCreate(full.Guid)),
            ["set_state"]=Message(new GameMessageSetState(full,(PhysicsState)0x12345678)),["parent"]=Message(new GameMessageParentEvent(full,child)),
            ["pickup"]=Message(new GameMessagePickupEvent(full)),["inventory_remove"]=Message(new GameMessageInventoryRemoveObject(full)),
            ["stack"]=Message(new GameMessageSetStackSize(full)),["teleport"]=Message(new GameMessagePlayerTeleport(new Player())),
        };
        using var restrictionBytes=new MemoryStream();using var restrictionWriter=new BinaryWriter(restrictionBytes);restrictionWriter.Write(new RestrictionDB(full));
        return new{models,physics,game,messages,restrictions=Convert.ToHexString(restrictionBytes.ToArray()),wave_command=unchecked((ushort)MotionCommand.Wave),run_command=unchecked((ushort)MotionCommand.RunForward)};
    }
}
// Domain adapters supply preselected authoritative projection fields. The seven
// object serialization method bodies are extracted byte-for-byte from the pin.
namespace ACE.Server.WorldObjects {
    public partial class WorldObject {
        public WeenieHeaderFlag Header; public WeenieHeaderFlag2 Header2;
        public PhysicsDescriptionFlag PhysicsHeader;
        public PhysicsState HarnessState;
        public ObjectDescriptionFlag ObjectDescriptionFlags;
        public ObjDesc Model = new();
        protected WeenieHeaderFlag CalculateWeenieHeaderFlag() => Header;
        protected WeenieHeaderFlag2 CalculateWeenieHeaderFlag2() => Header2;
        protected PhysicsDescriptionFlag CalculatedPhysicsDescriptionFlag() => PhysicsHeader;
        private PhysicsState GetPhysicsStateOrDefault() => HarnessState;
        public void UpdateObjectDescriptionFlags() { }
        public ObjDesc CalculateObjDesc() => Model;
        public CloakStatus CloakStatus;
        public uint WeenieClassId,IconId,ItemType;
        public string PluralName;
        public byte? ItemCapacity,ContainerCapacity;
        public ushort? AmmoType,Structure,MaxStructure,MaxStackSize,Script,HookType;
        public int? Value,StackSize,EncumbranceVal,CooldownId;
        public uint? ItemUseable,TargetType,UiEffects,ContainerId,WielderId,ValidLocations,CurrentWieldedLocation,ClothingPriority,SpellDID,HouseOwner,HookItemType,MonarchId,IconOverlayId,IconUnderlayId,MaterialType,PetOwner;
        public sbyte? CombatUse;
        public byte? RadarColor,RadarBehavior;
        public float? UseRadius,Workmanship;
        public double? CooldownDuration;
        public Motion CurrentMotionState;
        public uint MotionTableId,SoundTableId,PhysicsTableId,SetupTableId;
        public ParentLocation? ParentLocation;
        public List<HeldItem> Children = new();
        public float? ObjScale,Friction,Elasticity,Translucency,DefaultScriptIntensity;
        public Vector3 Velocity,Acceleration,Omega;
        public uint? DefaultScriptId;
        public byte[] ModelBytes() { using var ms=new MemoryStream();using var w=new BinaryWriter(ms);SerializeModelData(w);return ms.ToArray(); }
        public byte[] PhysicsBytes() { using var ms=new MemoryStream();using var w=new BinaryWriter(ms);SerializePhysicsData(w);return ms.ToArray(); }
    }
    public class SpellProjectile : WorldObject { }
    public sealed class HarnessLandblock { public bool IsDungeon; }
    public class House : WorldObject {
        public HouseType HouseType;public List<House> LinkedHouses=new();
        public House RootHouse => this;public HarnessLandblock CurrentLandblock=new();
        public bool OpenStatus;public uint HouseInstance;
        public Dictionary<ObjectGuid,bool> Guests=new();
    }
}
namespace ACE.Database.Models.Shard {
    public sealed class CharacterPropertiesFillCompBook { public uint SpellComponentId;public uint QuantityToRebuy; }
}
namespace ACE.Server.Managers {
    public sealed class HarnessOwner { public HarnessAccount Account=new(); }
    public sealed class HarnessAccount { public uint AccountId=1; public string AccountName="synthetic-account"; }
    public static partial class PlayerManager {
        public static WorldObject FindByGuid(uint guid)=>Players.TryGetValue(guid,out var player) ? player : new WorldObject{Guid=new ObjectGuid(guid),Name="Unknown synthetic"};
        public static Dictionary<uint,WorldObject> GetAccountPlayers(uint id)=>new();
    }
}
