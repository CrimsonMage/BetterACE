using ACE.Server.WorldObjects;
using ACE.Database.Models.World;
namespace ACE.Server.Factories {
 public static partial class LootGenerationFactory {
  public static string SpecialCase(string kind,int tier,double unit,float rate){ACE.Common.ThreadSafeRandom.Unit=unit;ACE.Common.ThreadSafeRandom.Draws=0;ACE.Server.Managers.PropertyManager.AetheriaRate=rate;
   if(kind=="pet"){var w=new PetDevice();MutatePetDevice(w,tier);return $"pet|{tier}|{unit:R}|{rate:R}|{ACE.Common.ThreadSafeRandom.Draws}|{w.GearDamage},{w.GearDamageResist},{w.GearCritDamage},{w.GearCritDamageResist},{w.GearCrit},{w.GearCritResist}|{w.ItemWorkmanship}";}
   var item=TryRollMundaneAddon(new TreasureDeath(){Tier=tier});return $"addon|{tier}|{unit:R}|{rate:R}|{ACE.Common.ThreadSafeRandom.Draws}|{item?.WeenieClassId}|{item?.ItemMaxLevel}|{item?.IconOverlayId}";
  }
 }
}
public static class Special {public static void Run(){foreach(var kind in new[]{"pet","addon"})foreach(var tier in Enumerable.Range(1,8))foreach(var unit in new[]{0.0,0.019,0.02,0.5,0.98})foreach(var rate in new[]{0.0f,1.0f,2.0f})Console.WriteLine(ACE.Server.Factories.LootGenerationFactory.SpecialCase(kind,tier,unit,rate));}}
