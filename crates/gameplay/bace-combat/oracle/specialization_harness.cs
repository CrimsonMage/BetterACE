// Only adapters/input construction are synthetic; method bodies are official.
using System;using System.Collections.Generic;using System.Globalization;
// SPELL_ENUM
enum Skill {MeleeDefense,MissileDefense,MagicDefense,Shield,Healing,SneakAttack,Deception,AssessPerson,HeavyWeapons,Recklessness}
enum SkillAdvancementClass {Inactive,Untrained,Trained,Specialized}
enum CombatType {Melee,Missile,Magic}
enum CombatMode {NonCombat,Melee,Missile,Magic}
class CreatureSkill {public uint Base,Current;public SkillAdvancementClass AdvancementClass;}
class WorldObject {public double? GetAbsorbMagicDamage()=>0.6;}
static class ThreadSafeRandom {public static float Roll;public static float Next(float min,float max)=>Roll;}
class Creature:WorldObject {
 public float Angle;public Dictionary<Skill,CreatureSkill> Skills=new();
 public CreatureSkill GetCreatureSkill(Skill skill)=>Skills[skill];public float GetAngle(WorldObject o)=>Angle;
 public Skill GetCurrentAttackSkill()=>Skill.HeavyWeapons;
 public Skill GetDefenseSkill(CombatType t)=>t==CombatType.Melee?Skill.MeleeDefense:t==CombatType.Missile?Skill.MissileDefense:Skill.MagicDefense;
 // DEFENSE
 // SNEAK
}
class Player:Creature {public CombatMode CombatMode;public float Power;public float GetPowerAccuracyBar()=>Power;public float GetDamageRating(int rating)=>(100+rating)/100.0f;
 // RECK
}
class SpellProjectile:WorldObject {
 // SHIELD
}
static class SkillCheck {public static int Effective,Difficulty;public static float GetSkillChance(int skill,int difficulty){Effective=skill;Difficulty=difficulty;return 0.5f;}}
class Healer {public int BoostValue;
 // HEAL
}
class Rating {public const float ArmorMod=200.0f/3.0f;
 // ARMOR
 // RATING
 // POSITIVE
}
class Program{
 static CreatureSkill S(int sac,uint value)=>new(){AdvancementClass=(SkillAdvancementClass)sac,Base=value,Current=value};
 static Player P(){var p=new Player();foreach(Skill s in Enum.GetValues<Skill>())p.Skills[s]=S(2,300);return p;}
 static void Main(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
 foreach(float armor in new[]{-100f,-1f,0f,1f,50f,100f,600f})Console.WriteLine($"armor,3,1000,{armor},{BitConverter.SingleToInt32Bits(Rating.CalcArmorMod(armor))}");
 foreach(int rating in new[]{0,1,5,10,60,100})Console.WriteLine($"rating,0,0,{rating},{BitConverter.SingleToInt32Bits(Rating.GetNegativeRatingMod(rating))}");
 Console.WriteLine($"dirty,0,0,{(int)SpellId.DF_Specialized_DefenseDebuff},{(int)SpellId.DF_Specialized_Bleed},{(int)SpellId.DF_Specialized_AttackDebuff},{(int)SpellId.DF_Specialized_HealingDebuff},{(int)SpellId.DF_Trained_DefenseDebuff},{(int)SpellId.DF_Trained_Bleed},{(int)SpellId.DF_Trained_AttackDebuff},{(int)SpellId.DF_Trained_HealingDebuff}");
 foreach(int sac in new[]{1,2,3})foreach(uint value in new uint[]{99,100,101,200,433,500})foreach(float angle in new[]{90f,91f}){
  var p=P();p.Angle=angle;p.Skills[Skill.Shield]=S(sac,value);var result=new SpellProjectile().GetShieldMod(p,new WorldObject());Console.WriteLine($"shield,{sac},{value},{angle},{BitConverter.SingleToInt32Bits(result)}");}
 foreach(int sac in new[]{1,2,3})foreach(uint value in new uint[]{49,50,59,60,305,600})foreach(CombatType type in Enum.GetValues<CombatType>()){
  var p=P();p.Skills[p.GetDefenseSkill(type)]=S(sac,value);Console.WriteLine($"defense,{sac},{value},{(int)type},{p.GetSpecDefenseBonus(type)}");}
 foreach(int sac in new[]{1,2,3})foreach(uint value in new uint[]{1,150,300})foreach(float power in new[]{0.09f,0.1f,0.5f,0.9f,0.91f}){
  var p=P();p.Skills[Skill.Recklessness]=S(sac,value);p.Power=power;p.CombatMode=CombatMode.Melee;Console.WriteLine($"reck,{sac},{value},{power:R},{BitConverter.SingleToInt32Bits(p.GetRecklessnessMod())}");}
 foreach(int sac in new[]{1,2,3})foreach(uint value in new uint[]{0,150,306})foreach(float angle in new[]{0f,90f,91f})foreach(float roll in new[]{0.0f,0.1f,0.15f}){
  var p=P();p.Skills[Skill.SneakAttack]=S(sac,value);p.Skills[Skill.Deception]=S(sac,value);var target=P();target.Angle=angle;target.Skills[Skill.AssessPerson]=S(2,153);ThreadSafeRandom.Roll=roll;Console.WriteLine($"sneak,{sac},{value},{angle},{roll:R},{BitConverter.SingleToInt32Bits(p.GetSneakAttackMod(target))}");}
 foreach(int sac in new[]{1,2,3})foreach(uint value in new uint[]{0,1,100,333})foreach(int boost in new[]{-5,0,10})foreach(bool combat in new[]{false,true}){
  var p=P();p.Skills[Skill.Healing]=S(sac,value);p.CombatMode=combat?CombatMode.Melee:CombatMode.NonCombat;var h=new Healer{BoostValue=boost};int difficulty=0;h.DoSkillCheck(p,p,17,ref difficulty);Console.WriteLine($"heal,{sac},{value},{boost},{(combat?1:0)},{SkillCheck.Effective},{difficulty}");}
 }}
