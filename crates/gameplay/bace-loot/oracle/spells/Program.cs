using ACE.Entity.Enum;
using ACE.Server.Factories.Enum;
using ACE.Server.Factories.Entity;
using ACE.Server.WorldObjects;
using ACE.Database.Models.World;
using System.Globalization;
namespace log4net {public interface ILog {void Error(string s);void Warn(string s);void Info(string s);}public class Log:ILog{public void Error(string s){}public void Warn(string s){}public void Info(string s){}}public static class LogManager{public static ILog GetLogger(Type t)=>new Log();}}
namespace ACE.Common {public static class ThreadSafeRandom {public static double Unit;public static int Draws;public static int Next(int a,int b){Draws++;return a+(int)(Unit*(b-a+1));}public static double Next(float a,float b){Draws++;return Unit*(b-a)+a;}public static double NextInterval(float q){Draws++;return Math.Max(0,Unit-q);}}}
namespace ACE.Database.Models.World {public class TreasureDeath {public int Tier,TreasureType=1;public float LootQualityMod;}}
namespace ACE.Entity.Models {public class FakeBiota {public Dictionary<int,float> PropertiesSpellBook=new();public void GetOrAddKnownSpell(int id,object db,out bool b){b=PropertiesSpellBook.TryAdd(id,2.0f);}}}
namespace ACE.Server.Managers {public static class PropertyManager {public static double AetheriaRate=1.0;public static (double Item,bool Other) GetDouble(string name)=>(name=="aetheria_drop_rate"?AetheriaRate:1.0,false);}}
namespace ACE.Server.Factories.Enum {public enum WeenieClassName {Undef,undef=0,ace42635_coalescedaetheria=42635,ace42637_coalescedaetheria=42637,ace42636_coalescedaetheria=42636,ace42516_coalescedmana=42516,ace42517_coalescedmana=42517,ace42518_coalescedmana=42518,capleather=45,glovescloth=121,orb=2366,flasksimple=7940}}
namespace ACE.Server.WorldObjects {
 public class WorldObject {public string Name="oracle";public ACE.Entity.Models.FakeBiota Biota=new();public object BiotaDatabaseLock;public uint WeenieClassId;public int? ItemMaxLevel;public uint? IconOverlayId;public int? GearDamage,GearDamageResist,GearCritDamage,GearCritDamageResist,GearCrit,GearCritResist;public int? ArmorLevel;public UiEffects UiEffects;public double? ManaRate;public uint? SpellDID;public int? ItemMaxMana,ItemCurMana,ItemSpellcraft,ItemWorkmanship,ItemDifficulty,ItemSkillLevelLimit;public Skill ItemSkillLimit;public Skill WeaponSkill;public DamageType W_DamageType;public bool IsShield;public CoverageMask? ClothingPriority;public WieldRequirement WieldRequirements,WieldRequirements2;public int? WieldDifficulty,WieldDifficulty2;}
 public class Gem:WorldObject{}public class PetDevice:WorldObject{}
}
namespace ACE.Server.Entity {public class Formula {public int Level;}public class Spell {public Formula Formula;public uint Power,BaseMana;public Spell(SpellId id):this((uint)id){}public Spell(int id):this((uint)id){}public Spell(uint id){Formula=new(){Level=(int)id%8+1};Power=id%401;BaseMana=id%31+1;}}}
namespace ACE.Server.Factories {
 public static class WorldObjectFactory {public static WorldObject CreateNewWorldObject(uint id)=>new(){WeenieClassId=id};}
 public partial class LootGenerationFactory {
  static readonly log4net.ILog log=new log4net.Log();
  public static string Run(string name,int tier,double unit,float quality,bool magic=false){
   WorldObject w=name=="gem"?new Gem():new WorldObject();var r=new TreasureRoll(){Wcid=(WeenieClassName)100};w.WeaponSkill=Skill.LightWeapons;w.WieldRequirements=WieldRequirement.Level;w.WieldDifficulty=100;w.WieldRequirements2=WieldRequirement.Level;w.WieldDifficulty2=150;
   switch(name){
    case "gem":r.ItemType=TreasureItemType.Gem;break;
    case "jewelry":r.ItemType=TreasureItemType.Jewelry;break;
    case "crown":r.ItemType=TreasureItemType.Jewelry;w.ArmorLevel=10;break;
    case "orb":r.ItemType=TreasureItemType.Caster;r.WeaponType=TreasureWeaponType.Caster;r.Wcid=WeenieClassName.orb;break;
    case "wand":r.ItemType=TreasureItemType.Caster;r.WeaponType=TreasureWeaponType.Caster;w.W_DamageType=DamageType.Fire;break;
    case "void":r.ItemType=TreasureItemType.Caster;r.WeaponType=TreasureWeaponType.Caster;w.W_DamageType=DamageType.Nether;break;
    case "melee":r.ItemType=TreasureItemType.Weapon;r.WeaponType=TreasureWeaponType.Axe;break;
    case "twohanded":r.ItemType=TreasureItemType.Weapon;r.WeaponType=TreasureWeaponType.TwoHandedAxe;w.WeaponSkill=Skill.TwoHandedCombat;break;
    case "missile":r.ItemType=TreasureItemType.Weapon;r.WeaponType=TreasureWeaponType.Bow;break;
    case "armor":r.ItemType=TreasureItemType.Armor;r.ArmorType=(TreasureArmorType)1;w.ArmorLevel=100;w.ClothingPriority=CoverageMask.OuterwearChest;break;
    case "shield":r.ItemType=TreasureItemType.Armor;r.ArmorType=(TreasureArmorType)1;w.ArmorLevel=100;w.IsShield=true;break;
    case "shirt":r.ItemType=TreasureItemType.Clothing;w.ClothingPriority=CoverageMask.UnderwearChest;break;
    case "gloves":r.ItemType=TreasureItemType.Clothing;r.Wcid=WeenieClassName.glovescloth;w.ArmorLevel=5;w.ClothingPriority=CoverageMask.Hands;break;
    case "cap":r.ItemType=TreasureItemType.Clothing;r.Wcid=WeenieClassName.capleather;w.ArmorLevel=5;w.ClothingPriority=CoverageMask.Head;break;
    case "dinnerware":r.ItemType=TreasureItemType.ArtObject;break;
    case "flask":r.ItemType=TreasureItemType.ArtObject;r.Wcid=WeenieClassName.flasksimple;break;
   }
   r.BaseArmorLevel=w.ArmorLevel??0;ACE.Common.ThreadSafeRandom.Unit=unit;ACE.Common.ThreadSafeRandom.Draws=0;
   if(magic){w.ItemWorkmanship=5;if(name=="orb"||name=="wand"||name=="void")w.SpellDID=50;AssignMagic(w,new TreasureDeath(){Tier=tier,LootQualityMod=quality},r);return $"{name}|{tier}|{unit:R}|{quality:R}|{ACE.Common.ThreadSafeRandom.Draws}|{r.ItemDifficulty:R}|{w.ItemMaxMana}|{w.ItemCurMana}|{w.ItemSpellcraft}|{w.ItemDifficulty}|{w.ItemSkillLevelLimit}|{(int)w.ItemSkillLimit}|{w.ManaRate:R}|{string.Join(",",w.Biota.PropertiesSpellBook.Keys)}";}
   var spells=RollSpells(w,new TreasureDeath(){Tier=tier,LootQualityMod=quality},r);
   return $"{name}|{tier}|{unit:R}|{quality:R}|{ACE.Common.ThreadSafeRandom.Draws}|{r.ItemDifficulty:R}|{w.WieldDifficulty}|{w.WieldDifficulty2}|{string.Join(",",spells.Distinct().Select(s=>(uint)s))}";
  }
 }
}
public class Program {public static void Main(string[] args){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;if(args.Length>0&&args[0]=="special"){Special.Run();return;}foreach(var name in new[]{"gem","jewelry","crown","orb","wand","void","melee","twohanded","missile","armor","shield","shirt","gloves","cap","dinnerware","flask"})foreach(var tier in new[]{1,4,6,8})foreach(var unit in new[]{0.0,0.1,0.5,0.98})foreach(var q in new[]{0.0f,0.2f})Console.WriteLine(ACE.Server.Factories.LootGenerationFactory.Run(name,tier,unit,q,args.Length>0));}}
