using ACE.Common;
using Server=ACE.Server;
using ACE.Entity.Enum;
using ACE.Server.Factories.Tables;
using ACE.Database.Models.World;
namespace log4net{public interface ILog{void Error(object v);}public class Log:ILog{public void Error(object v){}}public static class LogManager{public static ILog GetLogger(Type t)=>new Log();}}
namespace ACE.Common{public static class ThreadSafeRandom{public static double Value;public static int Draws;public static double Next(float min,float max){Draws++;return Value*(max-min)+min;}public static double NextInterval(float q){Draws++;return Math.Max(0,Value-q);}}}
namespace ACE.Database.Models.World{public class TreasureDeath{public int Tier;public float LootQualityMod;}}
namespace ACE.Server.Entity{public class Spell{public uint Level;public Spell(uint id){Level=id%8+1;}}}
public class WorldObject{public int? Value,ArmorLevel,ItemWorkmanship,ItemMaxMana,EncumbranceVal;public double? BulkMod,SizeMod;public MaterialType? MaterialType,GemType;public uint? SpellDID;public Biota Biota=new();public bool HasArmorLevel()=>ArmorLevel>0;}
public class Gem:WorldObject{}
public class Biota{public Dictionary<uint,float> PropertiesSpellBook=new();}
public class TreasureRoll{}
public class Oracle{
const float WeaponBulk=.5f,ArmorBulk=.25f,valueFactor=1f/3f,valueNonFactor=1f-valueFactor;
static readonly List<int> ItemValue_TierMod=new(){25,50,100,250,500,1000,2000,3000};
// METHODS
public static void Main(){
 foreach(var kind in new[]{"generic","armor","gem","weapon"})foreach(var tier in new[]{1,2,4,6,8})foreach(var draw in new[]{0d,.1d,.5d,.98d})foreach(var quality in new[]{0f,.2f}){
 WorldObject w=kind=="gem"?new Gem():new WorldObject();w.Value=101;w.ArmorLevel=kind=="armor"?123:null;w.ItemWorkmanship=7;w.ItemMaxMana=55;w.EncumbranceVal=123;w.BulkMod=.63;w.SizeMod=1.2;w.MaterialType=MaterialType.Diamond;w.GemType=MaterialType.Ruby;w.SpellDID=25;w.Biota.PropertiesSpellBook.Add(8,2);w.Biota.PropertiesSpellBook.Add(12,2);
 ACE.Common.ThreadSafeRandom.Value=draw;ACE.Common.ThreadSafeRandom.Draws=0;
 MutateBurden(w,new TreasureDeath{Tier=tier,LootQualityMod=quality},kind=="weapon");MutateValue(w,tier,new TreasureRoll());
 Console.WriteLine($"{kind}|{tier}|{draw:R}|{quality:R}|{w.Value}|{w.EncumbranceVal}|{ACE.Common.ThreadSafeRandom.Draws}");
 }
}}
