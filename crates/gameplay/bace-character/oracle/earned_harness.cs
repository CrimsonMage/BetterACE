using System;using System.Collections.Generic;using System.Linq;
class Table{public List<ulong> CharacterLevelXPList=new(){0,0,100,300};public List<uint> CharacterLevelSkillCreditList=new(){0,0,1,2};}
static class DatManager{public static Portal PortalDat=new();}class Portal{public Table XpTable=new();}
enum XpType{Quest,Allegiance}enum PropertyInt64{TotalExperience,AvailableExperience}enum PropertyInt{Level,AvailableSkillCredits}enum ChatMessageType{Broadcast,Advancement}enum PlayScript{WeddingBliss,LevelUp}
class GameMessagePrivateUpdatePropertyInt64{public GameMessagePrivateUpdatePropertyInt64(params object[] args){}}class GameMessagePrivateUpdatePropertyInt{public GameMessagePrivateUpdatePropertyInt(params object[] args){}}class GameMessageSystemChat{public GameMessageSystemChat(params object[] args){}}
class Network{public void EnqueueSend(params object[] values){}}class SessionType{public Network Network=new();}
class Fellow{public void OnFellowLevelUp(Player p){}}class Allegiance{public void OnLevelUp(){}}
class Player{
 public int? Level=1,AvailableSkillCredits=5,TotalSkillCredits=10;public long? TotalExperience=0,AvailableExperience=50;public SessionType Session=new();public bool HasVitae=false,Vitals=false;public Fellow Fellowship=null;public Allegiance AllegianceNode=null;public int Guid=1;
 void PlayParticleEffect(params object[] values){}void SetMaxVitals(){Vitals=true;}void UpdateXpVitae(long amount){}
 // MAXLEVEL
 // UPDATE
 // LEVELUP
 public void Run(long amount){UpdateXpAndLevel(amount,XpType.Quest);}
}
class Program{static void Main(){foreach(var initialTotal in new int?[]{10,null,-2})foreach(var level in new[]{1,2,3})foreach(long amount in new long[]{0,1,99,100,101,299,300,1000}){var p=new Player{TotalSkillCredits=initialTotal,Level=level,TotalExperience=level==1?0:level==2?100:300};p.Run(amount);Console.WriteLine($"{level},{amount},{p.Level},{p.TotalExperience},{p.AvailableExperience},{p.AvailableSkillCredits},{p.TotalSkillCredits?.ToString()??"null"},{p.Vitals},{initialTotal?.ToString()??"null"}");}}}
