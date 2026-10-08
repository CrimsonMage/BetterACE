using System.Globalization;
using ACE.Entity.Enum.Properties;
using ACE.Server.WorldObjects;
using ACE.Server.Entity.Mutations;
namespace log4net {
 public interface ILog {void Error(string text);}
 public class Log:ILog {public void Error(string text){}}
 public static class LogManager {public static ILog GetLogger(Type t)=>new Log();}
}
namespace ACE.Common {public static class ThreadSafeRandom {public static double Unit;public static int Draws;public static double Next(float a,float b){Draws++;return Unit*(b-a)+a;}}}
namespace ACE.Server.Factories.Enum {public enum Unused {}}
namespace ACE.Entity.Enum {public enum Skill {None=0,MeleeDefense=6,MissileDefense=7,MagicDefense=15,HeavyWeapons=44,LightWeapons=45,FinesseWeapons=46}public enum ImbuedEffectType {None=0}}
namespace ACE.Entity.Enum.Properties {
 public enum PropertyInt {EncumbranceVal=5,ArmorLevel=28,WieldRequirements2=270,WieldSkillType2=271,WieldDifficulty2=272,Damage=44,WeaponSkill=48,WieldRequirements=158,WieldSkillType=159,WieldDifficulty=160,ElementalDamageBonus=204,ImbuedEffect=179}
 public enum PropertyFloat {ArmorModVsSlash=13,ArmorModVsPierce=14,ArmorModVsBludgeon=15,ArmorModVsCold=16,ArmorModVsFire=17,ArmorModVsAcid=18,ArmorModVsElectric=19,DamageVariance=22,WeaponDefense=29,WeaponOffense=62,DamageMod=63,ManaConversionMod=144,ElementalDamageMod=152}
 public enum PropertyInt64 {DummyInt64=1}public enum PropertyBool {DummyBool=1}public enum PropertyDataId {DummyDataId=1}
}
namespace ACE.Server.WorldObjects {
 public class WorldObject {
  public string Name="oracle";public uint Guid=1;
  public SortedDictionary<int,int> Ints=new();public SortedDictionary<int,double> Floats=new();
  public int? GetProperty(PropertyInt p)=>Ints.TryGetValue((int)p,out var v)?v:null;
  public double? GetProperty(PropertyFloat p)=>Floats.TryGetValue((int)p,out var v)?v:null;
  public long? GetProperty(PropertyInt64 p)=>null;public bool? GetProperty(PropertyBool p)=>null;public uint? GetProperty(PropertyDataId p)=>null;
  public void SetProperty(PropertyInt p,int v)=>Ints[(int)p]=v;public void SetProperty(PropertyFloat p,double v)=>Floats[(int)p]=v;
  public void SetProperty(PropertyInt64 p,long v){}public void SetProperty(PropertyBool p,bool v){}public void SetProperty(PropertyDataId p,uint v){}
 }
}
public class Program {
 public static void Main(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
  var names=typeof(Program).Assembly.GetManifestResourceNames().Where(n=>n.StartsWith("ACE.Server.Entity.Mutations.")).OrderBy(n=>n,StringComparer.Ordinal);
  foreach(var resource in names){var name=resource.Replace("ACE.Server.Entity.Mutations.","");var filter=MutationCache.GetMutation(name);
   foreach(var tier in Enumerable.Range(1,9))foreach(var roll in new[]{0.0,0.099,0.5,0.999}){
    var item=new WorldObject();item.Ints[44]=100;item.Ints[48]=45;item.Floats[22]=0.2;item.Floats[29]=1.0;item.Floats[62]=1.0;
    ACE.Common.ThreadSafeRandom.Unit=roll;ACE.Common.ThreadSafeRandom.Draws=0;
    var mutated=filter.TryMutate(item,tier);
    var ints=string.Join(",",item.Ints.Select(p=>$"{p.Key}={p.Value}"));var floats=string.Join(",",item.Floats.Select(p=>$"{p.Key}={p.Value:R}"));
    Console.WriteLine($"{name}|{tier}|{roll:R}|{mutated}|{ACE.Common.ThreadSafeRandom.Draws}|{ints}|{floats}");
   }
  }
 }
}
