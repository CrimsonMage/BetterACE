using System;
using System.Collections.Generic;
using ACE.Database.Models.World;
using ACE.Server.Factories.Enum;
using ACE.Server.Factories.Tables;
using ACE.Server.Factories.Tables.Wcids;
using WeenieClassName=ACE.Server.Factories.Enum.WeenieClassName;
namespace log4net { public interface ILog{void Error(object v);} public class Log:ILog{public void Error(object v){}}public static class LogManager{public static ILog GetLogger(Type t)=>new Log();}}
namespace ACE.Common {public static class ThreadSafeRandom {public static double Value;public static int Count;public static double Next(float min,float max){Count++;return Value*(max-min)+min;}public static int Next(int min,int max){Count++;return min+(int)(Value*(max-min+1));}}}
namespace ACE.Database.Models.World {public class TreasureDeath {public int TreasureType,Tier,UnknownChances,ItemTreasureTypeSelectionChances,MagicItemTreasureTypeSelectionChances,MundaneItemTypeSelectionChances;public float LootQualityMod;}}
namespace ACE.Server.Factories.Entity {public class TreasureRoll{public TreasureItemType ItemType;public TreasureArmorType ArmorType;public TreasureWeaponType WeaponType;public WeenieClassName Wcid;public TreasureRoll(TreasureItemType item){ItemType=item;}}}
namespace ACE.Server.Factories {
using ACE.Server.Factories.Entity;
public static class Oracle {
static log4net.ILog log=new log4net.Log();
// ROUTING
// CATEGORY
public static void Main(){
 foreach(var tier in new[]{1,4,6,7,8})foreach(var heritage in new[]{0,1,19,20,22,24})foreach(var category in new[]{0,1,2})foreach(var profile in new[]{1,2,5,9,11,16,20,24,29})foreach(var random in new[]{0.0,0.099999,0.25,0.5,0.9,0.999999}){
 var p=new TreasureDeath{Tier=tier,UnknownChances=heritage,ItemTreasureTypeSelectionChances=profile,MagicItemTreasureTypeSelectionChances=profile,MundaneItemTypeSelectionChances=profile};
 ACE.Common.ThreadSafeRandom.Value=random;ACE.Common.ThreadSafeRandom.Count=0;
 var c=category==0?TreasureItemCategory.Item:category==1?TreasureItemCategory.MagicItem:TreasureItemCategory.MundaneItem;
 var v=RollWcid(p,c);
 Console.WriteLine($"{tier}|{heritage}|{category}|{profile}|{random:R}|{(int)(v?.ItemType??0)}|{(int)(v?.ArmorType??0)}|{(int)(v?.WeaponType??0)}|{(uint)(v?.Wcid??0)}|{ACE.Common.ThreadSafeRandom.Count}");
 }
}}
}
