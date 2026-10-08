// Synthetic world services around unmodified ACE methods. AGPL-3.0-only.
using System;using System.Collections.Generic;using System.Linq;using System.Globalization;using System.Threading;
enum DamageType { Undef,Nether,Health } enum CombatType { Magic }
[Flags] enum EnchantmentTypeFlags {Skill=16,Additive=32768,AttackSkills=65536,DefenseSkills=131072}
class PropertiesEnchantmentRegistry { public int SpellId; public int SpellCategory; public uint PowerLevel; public double StartTime; public uint CasterObjectId;public float StatModValue;public EnchantmentTypeFlags Flags; }
class Block {public Dictionary<uint,WorldObject> Entries=new();public WorldObject GetObject(uint id)=>Entries.GetValueOrDefault(id);}
class WorldObject {public Block CurrentLandblock=new();}
class Vital {public uint Current=100;}
class History {public void Add(WorldObject source,DamageType type,uint damage){} }
class Emote {public void OnDamage(Player p){} }
class Creature:WorldObject {
 public Vital Health=new();public bool IsDead=>Health.Current==0;public bool IsAlive=>!IsDead;public bool Invincible;
 public float Resist=1;public int DefenseRating;public int DotRating;public History DamageHistory=new();public Emote EmoteManager=new();
 public float GetResistanceMod(DamageType d,WorldObject source,object weapon)=>Resist;
 public float GetDamageResistRatingMod(CombatType t,bool use)=>GetNegativeRatingMod(DefenseRating);
 public int GetDotResistanceRating()=>DotRating;
 public void TakeDamageOverTime(float amount,DamageType d){Health.Current-=(uint)Math.Min(Health.Current,Math.Round(amount));}
 public void TakeDamageOverTime_NotifySource(Player p,DamageType d,float amount,bool a){}
 public static float AdditiveCombine(float a,float b)=>a+b-1.0f;
 // NEGATIVE
 // POSITIVE
}
class Player:Creature {public bool IsPKType=true;public int GetPKDamageResistRating()=>0;}
static class PropertyManager {public record Value(double Item);public static Value GetDouble(string key)=>new(1.0);}
static class Registry { public static HashSet<int> Level8AuraSelfSpells=new();
 // TOP
}
class Manager {
 public WorldObject WorldObject;public List<PropertiesEnchantmentRegistry> Entries=new();
 public List<PropertiesEnchantmentRegistry> GetEnchantments_TopLayer(EnchantmentTypeFlags f,uint key)=>Entries.Where(e=>(e.Flags&f)==f).ToList();
 // ADD
 // DEFENSE
 // ATTACK
 // DAMAGE
}
class Program {
 static void Main(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
  foreach(var (amount,rating,health,found) in new[]{(10f,0,100u,true),(10f,8,100u,true),(2.5f,0,100u,true),(50f,0,12u,true),(10f,0,100u,false)}){
   var target=new Creature{DefenseRating=rating};target.Health.Current=health;if(found)target.CurrentLandblock.Entries[2]=new Creature();
   var manager=new Manager{WorldObject=target};manager.ApplyDamageTick(new(){new(){CasterObjectId=2,StatModValue=amount}},DamageType.Undef);
   Console.WriteLine($"damage,{amount:R},{rating},{health},{(found?1:0)},{target.Health.Current}");
  }
  foreach(var amount in new[]{-10f,-15.5f,0f}){
   var manager=new Manager{WorldObject=new Creature(),Entries=new(){new(){StatModValue=amount,Flags=EnchantmentTypeFlags.Skill|EnchantmentTypeFlags.Additive|EnchantmentTypeFlags.AttackSkills},new(){StatModValue=amount,Flags=EnchantmentTypeFlags.Skill|EnchantmentTypeFlags.Additive|EnchantmentTypeFlags.DefenseSkills}}};
   Console.WriteLine($"debuff,{amount:R},{manager.GetAttackDebuffMod()},{manager.GetDefenseDebuffMod()}");
  }
  var entries=new List<PropertiesEnchantmentRegistry>{new(){SpellId=5939,SpellCategory=685,PowerLevel=100,StartTime=0,CasterObjectId=2},new(){SpellId=5943,SpellCategory=685,PowerLevel=100,StartTime=0,CasterObjectId=3}};
  Console.WriteLine($"top,{entries.GetEnchantmentsTopLayer(new ReaderWriterLockSlim(),new HashSet<int>())[0].CasterObjectId}");
  foreach(var rating in new[]{0,25,50,-20})Console.WriteLine($"healing,{rating},{Creature.GetNegativeRatingMod(rating):R}");
 }
}
