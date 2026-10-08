// Surrounding adapters only; methods and memberships are extracted verbatim.
using System; using System.Collections.Generic;
// ENUM
enum SkillAdvancementClass { Inactive,Untrained,Trained,Specialized }
enum PropertyInt { AvailableSkillCredits }
enum PropertyInt64 { AvailableExperience }
enum ChatMessageType { Broadcast }
static class Extensions { public static string ToSentence(this Skill skill) => skill.ToString(); }
class Network { public void EnqueueSend(params object[] args) {} }
class Session { public Network Network=new(); }
class GameMessagePrivateUpdateSkill { public GameMessagePrivateUpdateSkill(Player p,CreatureSkill s){} }
class GameMessagePrivateUpdatePropertyInt { public GameMessagePrivateUpdatePropertyInt(Player p,PropertyInt k,int v){} }
class GameMessagePrivateUpdatePropertyInt64 { public GameMessagePrivateUpdatePropertyInt64(Player p,PropertyInt64 k,long v){} }
class GameMessageSystemChat { public GameMessageSystemChat(string s,ChatMessageType t){} }
class SkillBase {public int TrainedCost=4, UpgradeCostFromTrainedToSpecialized=6;}
class SkillTable {public Dictionary<uint,SkillBase> SkillBaseHash=new();}
class XpTable {public List<uint> TrainedSkillXpList=new(){0,10,50,100};public List<uint> SpecializedSkillXpList=new(){0,5,20,60};}
class Portal {public SkillTable SkillTable=new(); public XpTable XpTable=new();}
static class DatManager {public static Portal PortalDat=new();}
class CreatureSkill {
 public Skill Skill; public SkillAdvancementClass AdvancementClass; public uint ExperienceSpent,InitLevel; public ushort Ranks;
 // BONUSES
}
class Player {
 public Session Session=new(); public CreatureSkill Record; public long? AvailableExperience=100; public int? AvailableSkillCredits=20;
 public int AugmentationSpecializeArmorTinkering,AugmentationSpecializeItemTinkering,AugmentationSpecializeMagicItemTinkering,AugmentationSpecializeWeaponTinkering,AugmentationSpecializeSalvaging;
 public int LumAugAllSkills=3,AugmentationSkilledMelee=1,AugmentationSkilledMissile=2,AugmentationSkilledMagic=3,Enlightenment=2,AugmentationJackOfAllTrades=1,LumAugSkilledSpec=4;
 public CreatureSkill GetCreatureSkill(Skill s,bool create=true)=>Record;
 // LISTS
 // METHODS
}
class Program {static void Main(){
 foreach(int id in new[]{6,14,18,28,29,30,40})foreach(int sac in new[]{2,3})foreach(int aug in new[]{0,1})foreach(int mode in new[]{0,1}){
  var p=new Player();p.Record=new(){Skill=(Skill)id,AdvancementClass=(SkillAdvancementClass)sac,ExperienceSpent=50,InitLevel=(uint)(sac==3?10:0),Ranks=2};
  p.AugmentationSpecializeArmorTinkering=p.AugmentationSpecializeItemTinkering=p.AugmentationSpecializeMagicItemTinkering=p.AugmentationSpecializeWeaponTinkering=p.AugmentationSpecializeSalvaging=aug;
  DatManager.PortalDat.SkillTable.SkillBaseHash[(uint)id]=new();
  bool ok=mode==1?p.ResetSkill((Skill)id):(sac==3?p.UnspecializeSkill((Skill)id,6):p.UntrainSkill((Skill)id,4));
  Console.WriteLine($"lower,{id},{sac},{aug},{mode},{(ok?1:0)},{(int)p.Record.AdvancementClass},{p.Record.ExperienceSpent},{p.Record.Ranks},{p.Record.InitLevel},{p.AvailableExperience},{p.AvailableSkillCredits}");
 }
 foreach(int id in new[]{1,2,4,6,10,13,18,31,40,43,44,47,49,54})foreach(int sac in new[]{0,1,2,3}){
  var p=new Player();var s=new CreatureSkill{Skill=(Skill)id,AdvancementClass=(SkillAdvancementClass)sac};
  Console.WriteLine($"bonus,{id},{sac},{s.GetAugBonus_Base(p)},{s.GetAugBonus_Current(p)}");
 }
 foreach(uint xp in new uint[]{0,5,20,60,100}){var p=new Player();p.Record=new(){Skill=Skill.MeleeDefense,AdvancementClass=SkillAdvancementClass.Trained,ExperienceSpent=xp};p.SpecializeSkill(Skill.MeleeDefense,6,false);Console.WriteLine($"overage,{xp},{p.Record.ExperienceSpent},{p.Record.Ranks},{p.Record.InitLevel}");}
}}
