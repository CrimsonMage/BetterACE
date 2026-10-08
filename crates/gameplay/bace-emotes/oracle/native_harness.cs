using System;
using System.Linq;
using System.Collections.Generic;
using System.Globalization;
enum EmoteCategory{Vendor=2,HeartBeat=5,Use=7,WoundedTaunt=15,HearChat=24,ReceiveTalkDirect=38}
enum VendorType{One=1,Two=2}
enum MotionStance:uint{NonCombat=0x8000003d}
enum MotionCommand:uint{Ready=0x40000003}
class PropertiesEmote{public int Id;public EmoteCategory Category;public string Quest;public VendorType? VendorType;public uint? WeenieClassId;public MotionStance? Style;public MotionCommand? Substyle;public float? MinHealth,MaxHealth;public float Probability;}
class Biota{public List<PropertiesEmote> PropertiesEmote=new();}
class WorldObject{public Biota Biota=new();public void GetCurrentMotionState(out MotionStance s,out MotionCommand m){s=MotionStance.NonCombat;m=MotionCommand.Ready;}}
class Health{public float Percent;}
class Creature:WorldObject{public Health Health=new();}
static class ThreadSafeRandom{public static double Value;public static double Next(float min,float max)=>Value;}
class EmoteManager{private WorldObject _worldObject;public WorldObject WorldObject=>_worldObject;public EmoteManager(WorldObject w){_worldObject=w;}/*METHOD*/}
class Program{
 static void Main(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;var c=new Creature();
 void Add(int id,int cat,float probability,string quest=null,uint? style=null,uint? motion=null,float? min=null,float? max=null,int? vendor=null){c.Biota.PropertiesEmote.Add(new PropertiesEmote{Id=id,Category=(EmoteCategory)cat,Probability=probability,Quest=quest,Style=(MotionStance?)style,Substyle=(MotionCommand?)motion,MinHealth=min,MaxHealth=max,VendorType=(VendorType?)vendor,WeenieClassId=(id%2==0?100u:200u)});}
 Add(1,7,.25f,"foo");Add(2,7,.75f,"foo");Add(3,7,.75f,"FOO");Add(4,7,1);Add(10,24,.5f);Add(11,24,.8f,"foo");Add(12,24,.2f,"other");Add(20,38,.4f,"ß");Add(21,38,.6f,"ss");Add(22,38,.8f,"ẞ");Add(23,38,.3f,"ı");Add(24,38,.7f,"I");Add(25,38,.6f,"σ");Add(30,5,.9f);Add(31,5,.4f,null,0x8000003d,0x40000003);Add(32,5,.2f,null,0x80000040);Add(40,15,.1f);Add(41,15,.5f,null,null,null,0,.5f);Add(42,15,.8f,null,null,null,.5f,1);Add(50,2,.5f,null,null,null,null,null,1);Add(51,2,.7f,null,null,null,null,null,2);
 var manager=new EmoteManager(c);string[] quests={null,"foo","FOO","other","ß","ẞ","ss","ı","I","i","İ","Σ","ς"};int[] cats={7,24,38,5,15,2};double[] draws={0,.25,.5,.9999999999999999};
 for(int category=0;category<cats.Length;category++)for(int q=0;q<quests.Length;q++)for(int draw=0;draw<draws.Length;draw++)for(int vendor=0;vendor<3;vendor++)for(int template=0;template<3;template++)for(int health=0;health<2;health++){ThreadSafeRandom.Value=draws[draw];c.Health.Percent=health==0?.5f:.8f;var result=manager.GetEmoteSet((EmoteCategory)cats[category],quests[q],vendor==0?null:(VendorType?)vendor,template==0?null:template==1?100u:777u,true);Console.WriteLine($"{category},{q},{draw},{vendor},{template},{health},{result?.Id??0}");}
 }
}
