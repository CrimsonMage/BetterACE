// ACE contributors; AGPL-3.0-only. Original source methods injected by generator.
using System;using System.Linq;using System.Collections.Generic;using ACE.Entity.Enum;using ACE.Common.Extensions;
namespace ACE.Common{public static class ThreadSafeRandom{public static int Draws;public static double Roll;public static int Next(int lo,int hi){Draws++;return lo+(int)(Roll*(hi-lo+1));}}}
static class ThreadSafeRandom{public static int Next(int l,int h)=>ACE.Common.ThreadSafeRandom.Next(l,h);}
static class PropertyManager{public static bool Drop;public static (bool Item,int Dummy) GetBool(string key)=>(Drop,0);}
static class EnumHelper{public static int NumFlags(uint v)=>System.Numerics.BitOperations.PopCount(v);}
class WorldObject{
 public DestinationType DestinationType;public BondedStatus Bonded;public uint Guid;public WeenieType WeenieType;public EquipMask? ValidLocations,CurrentWieldedLocation;public CoverageMask? ClothingPriority;public ParentLocation? ParentLocation;public CombatStyle? DefaultCombatStyle;public ItemType ItemType;public int? ArmorLevel,AmmoType;public bool AutoWieldLeft,IsShield;public int WeaponSkill;public string Name=>"synthetic";public int PlacementPosition;
 public bool IsAmmoLauncher=>DefaultCombatStyle==CombatStyle.Bow||DefaultCombatStyle==CombatStyle.Crossbow||DefaultCombatStyle==CombatStyle.Atlatl;
 public bool IsRanged=>IsAmmoLauncher||DefaultCombatStyle==CombatStyle.ThrownWeapon;public bool IsTwoHanded=>WeaponSkill==41;
}
class Creature{
 public bool IsNPC;public CombatStyle AiAllowedCombatStyle;public Dictionary<uint,WorldObject> Inventory=new(),EquippedObjects=new();
 public List<WorldObject> GetInventoryItemsOfTypeWeenieType(WeenieType t)=>Inventory.Values.Where(i=>i.WeenieType==t).OrderBy(i=>i.PlacementPosition).ToList();
 public bool TryRemoveFromInventory(uint id){if(!Inventory.Remove(id,out var i))return false;foreach(var old in Inventory.Values.Where(o=>o.PlacementPosition>i.PlacementPosition))old.PlacementPosition--;return true;}
 public bool TryAddToInventory(WorldObject item){foreach(var old in Inventory.Values)old.PlacementPosition++;item.PlacementPosition=0;Inventory.Add(item.Guid,item);return true;}
 public bool TryWieldObjectWithBroadcasting(WorldObject i,EquipMask l)=>TryWieldObject(i,l);
 public bool TryWieldObject(WorldObject item,EquipMask location){GetPlacementLocation(item,location,out var placement,out var parent);bool used=EquippedObjects.Values.Any(i=>IsWeaponSlot(location)?i.ParentLocation!=null&&i.ParentLocation==parent&&i.CurrentWieldedLocation!=EquipMask.MissileAmmo:item.WeenieType==WeenieType.Clothing?i.ClothingPriority!=null&&(i.ClothingPriority&(item.ClothingPriority??0))!=0:i.CurrentWieldedLocation!=null&&(i.CurrentWieldedLocation&location)!=0);if(used)return false;item.CurrentWieldedLocation=location;item.ParentLocation=parent;EquippedObjects[item.Guid]=item;return true;}
 public List<WorldObject> Drops(){var results=new List<WorldObject>();
 // DROP
 }
 // PLACEMENT
 // SLOTS
 // METHODS
}
class Program{
 static string Encode(WorldObject w)=>$"{w.Guid}:{(uint)w.WeenieType}:{(uint)(w.ValidLocations??0)}:{(uint)(w.ClothingPriority??0)}:{w.ArmorLevel??0}:{(int)(w.DefaultCombatStyle??0)}:{w.AmmoType??0}:{(w.AutoWieldLeft?1:0)}:{(w.IsShield?4:0)}:{(uint)w.ItemType}:{w.WeaponSkill}";
 static WorldObject Item(uint id,int kind,int location,int coverage,int armor,int style,int ammo=0,bool left=false,bool shield=false,int skill=45)=>new(){Guid=id,WeenieType=(WeenieType)kind,ValidLocations=(EquipMask)location,ClothingPriority=(CoverageMask)coverage,ArmorLevel=armor,DefaultCombatStyle=(CombatStyle)style,AmmoType=ammo,AutoWieldLeft=left,IsShield=shield,ItemType=shield?ItemType.Armor:ItemType.MeleeWeapon,WeaponSkill=skill};
 static void Main(){foreach(bool enabled in new[]{false,true}){PropertyManager.Drop=enabled;var c=new Creature();uint id=1;foreach(int destination in new[]{2,10,0,8})foreach(int bonded in new[]{-2,-1,0,1,2}){var w=new WorldObject{Guid=id++,DestinationType=(DestinationType)destination,Bonded=(BondedStatus)bonded};c.Inventory[w.Guid]=w;}Console.WriteLine($"DROP|{(enabled?1:0)}|{string.Join(';',c.Inventory.Values.Select(i=>$"{i.Guid}:{(int)i.DestinationType}:{(int)i.Bonded}"))}|{string.Join(';',c.Drops().Select(i=>i.Guid))}");}foreach(int count in new[]{1,2,3,8,16,17,31,65})foreach(double roll in new[]{0.0,0.37,0.999})foreach(int scenario in new[]{0,1,2,3,4}){
 var c=new Creature{AiAllowedCombatStyle=CombatStyle.DualWield,IsNPC=scenario==3};var all=new List<WorldObject>();for(int i=0;i<count;i++){WorldObject item=scenario switch{0=>Item((uint)i+1,2,new[]{1,0x202,0x808,0x1010,0x20,0x2040,0x400,0x4080,0x100}[i%9],new[]{0x4000,0x400,0x1400,0x2000,0x8000,0x100,0x800,0x200,0x10000}[i%9],i%4*10,0),1 or 4=>Item((uint)i+1,6,i%3==0?0x200000:0x100000,0,0,2,0,i%3==0),2=>i%3==0?Item((uint)i+1,3,0x400000,0,0,16,1):i%3==1?Item((uint)i+1,5,0x800000,0,0,0,1):Item((uint)i+1,2,0x200000,0,0,0,0,false,true),_=>Item((uint)i+1,2,new[]{2,64,0x8000000}[i%3],new[]{8,2,0}[i%3],0,0)};all.Add(item);c.TryAddToInventory(item);}
 ACE.Common.ThreadSafeRandom.Roll=roll;ACE.Common.ThreadSafeRandom.Draws=0;c.EquipInventoryItems();if(scenario==4){for(int i=0;i<3;i++){var added=Item((uint)count+1+(uint)i,6,i==1?0x200000:0x100000,0,0,2,0,i==1);all.Add(added);c.TryAddToInventory(added);}}c.EquipInventoryItems();var output=string.Join(';',all.Where(i=>c.Inventory.ContainsKey(i.Guid)||c.EquippedObjects.ContainsKey(i.Guid)).Select(i=>$"{i.Guid}:{(uint)(i.CurrentWieldedLocation??0)}"));Console.WriteLine($"{count}|{roll:R}|{scenario}|{string.Join(';',all.Select(Encode))}|{ACE.Common.ThreadSafeRandom.Draws}|{output}");
 }}
}
