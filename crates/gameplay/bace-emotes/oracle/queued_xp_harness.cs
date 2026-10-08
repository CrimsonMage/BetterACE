using System;using System.Collections.Generic;using System.Linq;
class WorldObject{public int Guid=1,WeenieClassId=1;public string Name="fixture";}
class PropertiesEmoteAction{public uint Type;public float Delay;public string Message="";}
class PropertiesEmote{public int Category;public string Quest="";public List<PropertiesEmoteAction> PropertiesEmoteAction=new();}
class Logger{public void Error(string s){}public void Warn(string s){}}
enum EmoteType{Nop=0}
enum XpType{Quest,Allegiance,Kill,Fellowship,Emote}
[Flags]enum ShareType{None=0,Fellowship=1,Allegiance=2,All=3}
enum PropertyInt64{TotalExperience,AvailableExperience}enum PropertyInt{Level,AvailableSkillCredits}enum ChatMessageType{Broadcast,Advancement}enum PlayScript{WeddingBliss,LevelUp}
static class OwnerQueue{public static Queue<Action> Work=new();}
class ActionEventDelegate{public Action Run;public ActionEventDelegate(Action run){Run=run;}}
class ActionChain{List<Action> actions=new();public void AddDelaySeconds(double seconds){if(seconds!=0)throw new Exception("zero-delay fixture");}public void AddAction(WorldObject owner,Action action){actions.Add(action);}public void EnqueueChain(){OwnerQueue.Work.Enqueue(()=>{foreach(var a in actions)a();});}}
class Table{public List<ulong> CharacterLevelXPList=new(){0,0,100,300};public List<uint> CharacterLevelSkillCreditList=new(){0,0,0,0};}
static class DatManager{public static Portal PortalDat=new();}class Portal{public Table XpTable=new();}
static class PropertyManager{public static double Global=1,Quest=1;public static (double Item,int X) GetDouble(string key)=>(key=="xp_modifier"?Global:Quest,0);}
class GameMessagePrivateUpdatePropertyInt64{public GameMessagePrivateUpdatePropertyInt64(params object[] args){}}class GameMessagePrivateUpdatePropertyInt{public GameMessagePrivateUpdatePropertyInt(params object[] args){}}class GameMessageSystemChat{public GameMessageSystemChat(params object[] args){}}
class Network{public void EnqueueSend(params object[] values){}}class SessionType{public Network Network=new();}
class Fellow{public bool ShareXP=false;public void SplitXp(ulong amount,XpType type,ShareType share,Player player){throw new Exception("unsupported fellowship branch");}public void OnFellowLevelUp(Player p){}}
class Allegiance{public void OnLevelUp(){}}
class Item{public bool HasItemLevel=false;}
class Player:WorldObject{
 public int? Level=1,AvailableSkillCredits=0,TotalSkillCredits=0;public long? TotalExperience=0,AvailableExperience=100;public SessionType Session=new();public bool IsOlthoiPlayer=false,HasVitae=false;public Fellow Fellowship=null;public Allegiance AllegianceNode=null;Logger log=new();public float XpModifier=1;public Dictionary<uint,Item> EquippedObjects=new();
 public void EnqueueAction(ActionEventDelegate action){OwnerQueue.Work.Enqueue(action.Run);}
 float GetXPAndLuminanceModifier(XpType type)=>XpModifier;
 void PlayParticleEffect(params object[] values){}void SetMaxVitals(){}void UpdateXpVitae(long amount){throw new Exception("unsupported vitae");}void UpdateXpAllegiance(long amount){throw new Exception("unsupported allegiance");}void GrantItemXP(Item item,long amount){throw new Exception("unsupported item XP");}
 // MAXLEVEL
 // UPDATE
 // LEVELUP
 // EARN
 // GRANT
 // ITEMXP
}
class Manager{
 public bool IsBusy;public int Nested;bool Debug=false;WorldObject WorldObject=new();Logger log=new();public Player Player;public string Case;public List<string> Trace=new();
 bool EmoteIsBranchingType(PropertiesEmoteAction e)=>false;
 float ExecuteEmote(PropertiesEmote set,PropertiesEmoteAction action,WorldObject target){
 if(action.Type==62)Player.EarnXP(120,XpType.Quest,ShareType.None);
 if(action.Type==115)Player.TotalExperience=80;
 if(action.Type==114)Trace.Add($"{Case},query,{Player.Level},{Player.TotalExperience},{Player.AvailableExperience}");return 0;
 }
 // EXECUTE
 // ENQUEUE
 // DO_ENQUEUE
}
class Program{static void Main(){foreach(var change in new[]{false,true}){
 OwnerQueue.Work.Clear();PropertyManager.Global=2;PropertyManager.Quest=0.5;var player=new Player{XpModifier=1.25f};var manager=new Manager{Player=player,Case=change?"intervening":"plain"};var set=new PropertiesEmote();set.PropertiesEmoteAction.Add(new(){Type=62});if(change)set.PropertiesEmoteAction.Add(new(){Type=115});set.PropertiesEmoteAction.Add(new(){Type=114});manager.ExecuteEmoteSet(set,player);if(manager.IsBusy)throw new Exception("outer incorrectly held busy");foreach(var line in manager.Trace)Console.WriteLine(line);
 // The original closure has already captured the modified amount; later rate changes cannot reroll it.
 PropertyManager.Global=10;player.XpModifier=3;
 while(OwnerQueue.Work.Count>0)OwnerQueue.Work.Dequeue()();Console.WriteLine($"{manager.Case},recipient,{player.Level},{player.TotalExperience},{player.AvailableExperience}");
 }
 DatManager.PortalDat.XpTable.CharacterLevelXPList=new(){0,0,9000000000000000000,9100000000000000000};
 foreach(var row in new[]{(Amount:16777217L,Global:1.0),(Amount:13L,Global:0.5),(Amount:15L,Global:0.5)}){
 OwnerQueue.Work.Clear();PropertyManager.Global=row.Global;PropertyManager.Quest=1;var p=new Player();p.EarnXP(row.Amount,XpType.Quest,ShareType.None);while(OwnerQueue.Work.Count>0)OwnerQueue.Work.Dequeue()();Console.WriteLine($"scaled,{row.Amount},{row.Global},{p.TotalExperience}");
 }
 }}
