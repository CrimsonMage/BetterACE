using System;
using System.Globalization;
using ACE.Server.WorldObjects;
using ACE.Entity.Enum;
namespace ACE.Entity.Enum {public enum XpType{Quest,Kill} [Flags]public enum ShareType{None=0,Fellowship=1,All=3}public enum ChatMessageType{Broadcast}public enum CombatMode{NonCombat=1,Melee=2,Missile=4,Magic=8}}
namespace ACE.Entity.Enum.Properties {public enum PropertyInt64{AvailableLuminance}}
namespace ACE.Server.Managers {public static class PropertyManager {public static double Lum=1,Quest=1;public static (double Item,string) GetDouble(string key)=>(key=="luminance_modifier"?Lum:Quest,"");}}
namespace ACE.Server.Network.GameMessages.Messages {public class GameMessageSystemChat{public GameMessageSystemChat(string s,ChatMessageType t){}}public class GameMessagePrivateUpdatePropertyInt64{public GameMessagePrivateUpdatePropertyInt64(Player p,ACE.Entity.Enum.Properties.PropertyInt64 k,long v){}}}
namespace ACE.Server.WorldObjects {
 public class WorldObject{public int? ItemLevel;}
 public class Creature:WorldObject{public CombatMode CombatMode;}
 public class Network{public void EnqueueSend(object v){}}
 public class Session{public Network Network=new Network();}
 public class Fellowship{public bool ShareXP;public void SplitLuminance(ulong amount,XpType x,ShareType s,Player p)=>throw new Exception("sharing not invoked by helper vectors");}
 public partial class Player:Creature{public bool IsOlthoiPlayer;public long? AvailableLuminance,MaximumLuminance;public Fellowship Fellowship;public Session Session=new Session();public double Modifier=1;public int LumAugSurgeChanceRating;public double GetXPAndLuminanceModifier(XpType x)=>Modifier;}
}
static class ProcOracle {/*PROC_METHOD*/}
class Program {
 static void Main(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
 foreach(long available in new long[]{0,100,999,1000})foreach(long amount in new long[]{0,1,2,100,2000})foreach(double modifier in new[]{.5,1.0,2.0})foreach(double enchant in new[]{1.0,1.25}){
  ACE.Server.Managers.PropertyManager.Lum=modifier;ACE.Server.Managers.PropertyManager.Quest=1.5;var p=new Player{AvailableLuminance=available,MaximumLuminance=1000,Modifier=enchant};p.EarnLuminance(amount,XpType.Quest,ShareType.None);Console.WriteLine($"lum,{available},{amount},{modifier},{enchant},{p.AvailableLuminance}");
  p.AvailableLuminance=available;bool success=p.SpendLuminance(amount);Console.WriteLine($"spend,{available},{amount},{success},{p.AvailableLuminance}");
 }
 foreach(ItemXpStyle style in Enum.GetValues<ItemXpStyle>())foreach(int level in new[]{0,1,2,3,4,5})Console.WriteLine($"itemxp,{(int)style},{level},{ACE.Server.Entity.ExperienceSystem.ItemLevelToTotalXP(level,100,5,style)}");
 foreach(ItemXpStyle style in Enum.GetValues<ItemXpStyle>())foreach(ulong xp in new ulong[]{100,199,200,299,300,499,500,600,700,1500,3100})Console.WriteLine($"itemlevel,{(int)style},{xp},{ACE.Server.Entity.ExperienceSystem.ItemTotalXPToLevel(xp,100,5,style)}");
 foreach(int level in new[]{0,1,2,3,4,5})foreach(int aug in new[]{0,1,5,10})foreach(int mode in new[]{1,2,4,8}){var p=new Player{CombatMode=(CombatMode)mode,LumAugSurgeChanceRating=aug};float rate=ProcOracle.CalcProcRate(new WorldObject{ItemLevel=level},p);Console.WriteLine($"proc,{level},{aug},{mode},{BitConverter.SingleToUInt32Bits(rate)}");}
 }
}
