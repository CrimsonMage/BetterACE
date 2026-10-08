// Independent fixture harness for unchanged ACE methods in Extracted.cs.
using System.Globalization;
[Flags] public enum RegenLocationType {Treasure=64}
public static class ThreadSafeRandom {public static double Unit;public static double Next(float a,float b)=>a+Unit*(b-a);}
public partial class SourceProfile {public float Probability;public int InitCreate,MaxCreate;public uint WeenieClassId;}
public partial class GeneratorProfile {
 public SourceProfile Biota=new();public int Queued,Spawned;public bool IsAvailable=true;public bool GeneratedTreasureItem;
 public bool IsPlaceholder=>Biota.WeenieClassId==3666; public int CurrentCreate=>GeneratedTreasureItem?(Queued+Spawned>0?1:0):Queued+Spawned;
 public int InitCreate=>Biota.InitCreate;public int MaxCreate=>Biota.MaxCreate;public bool IsMaxed=>MaxCreate!=-1&&CurrentCreate>=MaxCreate;
 public RegenLocationType RegenLocationType;public uint LinkId=0;public uint WeenieClassId=>Biota.WeenieClassId;
 public void Enqueue(int n){for(int i=0;i<n;i++)Queued++;}
}
public class Logger {public void Warn(string text) {}}
public class Loc {public string ToLOCString()=>"";}
public partial class WorldObject {
 public List<GeneratorProfile> GeneratorProfiles=new();public bool CurrentlyPoweringUp;public int InitCreate,MaxCreate;
 public int CurrentCreate=>GeneratorProfiles.Sum(p=>p.CurrentCreate);
 public bool AllProfilesMaxed=>!GeneratorProfiles.Any(p=>!p.IsPlaceholder&&!p.IsMaxed);
 public bool AllProfilesUnavailable=>!GeneratorProfiles.Any(p=>!p.IsPlaceholder&&p.IsAvailable);
 public Logger log=new();public uint Guid=1,WeenieClassId=1;public string Name="oracle";public ACE.Entity.Position Location=new(0x01010001,10,20,30,0,0,0,1);
}
public class Program {
 static GeneratorProfile P(float prob,int init=1,int max=1,int spawned=0,bool avail=true,uint wcid=10,bool treasure=false,bool generated=false)=>new(){Biota=new(){Probability=prob,InitCreate=init,MaxCreate=max,WeenieClassId=wcid},Spawned=spawned,IsAvailable=avail,RegenLocationType=treasure?RegenLocationType.Treasure:0,GeneratedTreasureItem=generated};
 static void Case(string name,int init,int max,bool powering,double unit,params GeneratorProfile[] p){
  var g=new WorldObject(){InitCreate=init,MaxCreate=max,CurrentlyPoweringUp=powering,GeneratorProfiles=p.ToList()};
  string before=string.Join(",",p.Select(x=>x.CurrentCreate));var total=g.GetTotalProbability();var adjusted=string.Join(",",Enumerable.Range(0,p.Length).Select(i=>g.GetAdjustedProbability(i).ToString("R",CultureInfo.InvariantCulture)));
  ThreadSafeRandom.Unit=unit;g.SelectAProfile();
  Console.WriteLine($"{name}|{total:R}|{adjusted}|{string.Join(",",p.Select(x=>x.Queued))}|{string.Join(",",p.Select(x=>x.InitCreate))}|{string.Join(",",p.Select(x=>x.MaxCreate))}");
 }
 public static void Main(string[] args){if(args.Length>0){if(args[0]=="status")StatusCases.Run();else PlacementCases.Run();return;}CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
  Case("threshold",1,2,false,.5,P(.99f),P(1f));
  Case("maxed_threshold",1,2,false,.5,P(.99f,spawned:1),P(1f));
  Case("unavailable_threshold",1,2,false,.5,P(.99f,avail:false),P(1f));
  Case("reset_threshold",1,3,false,.75,P(.6f),P(.2f),P(.8f));
  Case("unconditional",3,4,true,.999,P(-1),P(-1),P(.5f),P(1f));
  Case("unconditional_capped",1,1,true,.9,P(-1),P(-1));
  Case("placeholder_probability",1,1,true,.1,P(.8f,wcid:3666),P(1f));
  Case("batch_exceeds_initial",1,8,true,.5,P(-1,5,8));
  Case("infinite_max",3,9,true,.5,P(-1,5,-1));
  Case("infinite_initial",3,9,true,.5,P(-1,-1,5));
  Case("treasure_clamp",3,9,true,.5,P(-1,4,8,treasure:true));
  Case("treasure_occupancy",3,9,true,.5,P(-1,1,1,4,treasure:true,generated:true),P(1f));
  Case("zero_initial",1,1,true,.5,P(-1,0,1));
  Case("zero_probability",1,1,true,0,P(0));
  Case("partial_capacity",1,3,false,.5,P(-1,5,8,2));
 }
}
