using System;using System.Numerics;using ACE.Entity.Enum;using ACE.Entity.Enum.Properties;
class WorldObject{
 public WeenieType WeenieType=WeenieType.Generic;public string PluralName;
 public int? ItemCapacity,ContainerCapacity,AmmoType,Value,ItemUseable,TargetType,UiEffects,CombatUse,Structure,MaxStructure,StackSize,MaxStackSize,ValidLocations,CurrentWieldedLocation,ClothingPriority,RadarColor,RadarBehavior,EncumbranceVal,HookType,MaterialType,CooldownId,HookItemType;
 public uint? ContainerId,WielderId,SpellDID,HouseOwner,MonarchId,IconOverlayId,IconUnderlayId,PetOwner,PhysicsScript,ParentLocation,DefaultScriptId;
 public float? UseRadius,Workmanship,ObjScale,Friction,Elasticity,Translucency,DefaultScriptIntensity;public double? CooldownDuration;
 public bool IsLocked,Inscribable,Stuck,HiddenAdmin,UiHidden,IgnoreHouseBarriers,RequiresPackSlot,Retained,WieldOnUse,WieldLeft;public bool Attackable=true;
 public CloakStatus CloakStatus;public PlayerKillerStatus PlayerKillerStatus;public ObjectDescriptionFlag ObjectDescriptionFlags=ObjectDescriptionFlag.Attackable;
 public object CurrentMotionState,Location;public int? Placement;public uint MotionTableId,SoundTableId,PhysicsTableId,SetupTableId;public System.Collections.Generic.List<int>Children=new();public Vector3 Velocity,Acceleration,Omega;
 public uint? GetProperty(PropertyDataId key)=>PhysicsScript;public int? GetProperty(PropertyInt key)=>HookItemType;
 FLAGS1
 FLAGS2
 DESCRIPTION
 UPDATE
 PHYSICS
 public string Flags()=>((uint)CalculateWeenieHeaderFlag())+","+((uint)CalculateWeenieHeaderFlag2())+","+Description()+","+((uint)CalculatedPhysicsDescriptionFlag());
 uint Description(){UpdateObjectDescriptionFlags();return(uint)ObjectDescriptionFlags;}
}
class Creature:WorldObject{}class Player:Creature{}class SlumLord:Creature{}class SpellProjectile:WorldObject{}class House:WorldObject{public HouseType HouseType;public House[]LinkedHouses;}
static class PropertyManager{public static (bool Item,string Description)GetBool(string key)=>(false,"");}
class Program{
 static void Main(){for(int c=0;c<8;c++){WorldObject w=c==4?new Player():c==5?new SlumLord():new WorldObject();
 if(c!=0){foreach(var f in typeof(WorldObject).GetFields()){if(f.FieldType==typeof(int?))f.SetValue(w,c==2?0:9);if(f.FieldType==typeof(uint?))f.SetValue(w,c==2?0u:9u);if(f.FieldType==typeof(float?))f.SetValue(w,c==2?0f:1.5f);if(f.FieldType==typeof(double?))f.SetValue(w,c==2?0.0:1.5);if(f.FieldType==typeof(bool))f.SetValue(w,true);}w.PluralName="Things";w.MotionTableId=1;w.SoundTableId=2;w.PhysicsTableId=3;w.SetupTableId=4;}
 if(c==2)w.Workmanship=null;
 if(c==3){w.ObjScale=.0009f;w.Translucency=-.0009f;w.CooldownDuration=.0009;w.Value=-9;w.Workmanship=1.5f;w.WielderId=0;}
 if(c==4){w.WeenieType=WeenieType.Creature;w.ObjectDescriptionFlags|=ObjectDescriptionFlag.Player;}
 if(c==5)w.WeenieType=WeenieType.SlumLord;
 if(c==6){w.WeenieType=WeenieType.Admin;w.CloakStatus=CloakStatus.Player;w.PlayerKillerStatus=PlayerKillerStatus.PK;}
 if(c==7){w.WeenieType=WeenieType.Chest;w.IsLocked=false;w.PlayerKillerStatus=PlayerKillerStatus.PKLite;w.Velocity=new Vector3(1,2,3);w.CurrentMotionState=new object();w.Location=new object();w.Children.Add(1);}
 Console.WriteLine(c+","+w.Flags());}}
}
