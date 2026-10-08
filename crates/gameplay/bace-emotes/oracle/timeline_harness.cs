using System;using System.Linq;using System.Collections.Generic;
class PropertiesEmoteAction {public uint Type;public float Delay;public string Message="";public float Post;}
class PropertiesEmote {public int Category;public string Quest="";public List<PropertiesEmoteAction> PropertiesEmoteAction=new();}
class WorldObject {public int Guid=1,WeenieClassId=1;public string Name="fixture";}
class Logger {public void Error(string s){}}
enum EmoteType {Nop=0}
static class Clock {public static double Now;public static long Seq;public static List<(double,long,Action)> Q=new();public static void Add(double delay,Action action){Q.Add((Now+delay,++Seq,action));}public static void Run(double time){while(Q.Count>0){var next=Q.OrderBy(x=>x.Item1).ThenBy(x=>x.Item2).First();if(next.Item1>time)break;Q.Remove(next);Now=Math.Max(Now,next.Item1);next.Item3();}Now=time;}}
class ActionChain {double delay;List<Action> actions=new();public void AddDelaySeconds(double v){delay+=v;}public void AddAction(WorldObject owner,Action action){actions.Add(action);}public void EnqueueChain(){Clock.Add(delay,()=>{foreach(var a in actions)a();});}}
class Manager {
 public bool IsBusy;public int Nested;bool Debug=false;WorldObject WorldObject=new();Logger log=new();public string Name="";public PropertiesEmote Child=new();public List<string> Trace=new();
 bool EmoteIsBranchingType(PropertiesEmoteAction e)=>e.Type==67;
 float ExecuteEmote(PropertiesEmote set,PropertiesEmoteAction e,WorldObject target){Trace.Add($"{Name},{e.Type},{Clock.Now:R}");if(e.Type==67)ExecuteEmoteSet(Child,target,true);if(e.Type==3)Clock.Add(0,()=>Trace.Add($"{Name},103,{Clock.Now:R}"));return e.Post;}
 // EXECUTE
 // ENQUEUE
 // DO_ENQUEUE
}
class Program{
 static PropertiesEmote Set(params (uint,float,float)[] rows){var s=new PropertiesEmote();foreach(var r in rows)s.PropertiesEmoteAction.Add(new(){Type=r.Item1,Delay=r.Item2,Post=r.Item3});return s;}
 static void Main(){
 foreach(var delay in new[]{0f,0.25f,1f})foreach(var post in new[]{0f,0.5f,2f})foreach(var late in new[]{false,true}){
 Clock.Now=0;Clock.Q.Clear();Clock.Seq=0;var m=new Manager{Name=$"basic:{delay:R}:{post:R}:{late}",Child=Set((8u,0.25f,0f))};m.ExecuteEmoteSet(Set((1u,delay,post),(67u,delay,0f),(10u,delay,post)),null);
 if(late)Clock.Now=3;
 Clock.Run(30);foreach(var line in m.Trace)Console.WriteLine(line);Console.WriteLine($"{m.Name},busy,{m.IsBusy},{m.Nested}");}
 Clock.Now=0;Clock.Q.Clear();Clock.Seq=0;var nested=new Manager{Name="nested_zero",Child=Set((8u,0f,0f),(10u,0f,0f))};nested.ExecuteEmoteSet(Set((67u,0f,0f),(1u,0f,0f)),null);Clock.Run(0);foreach(var line in nested.Trace)Console.WriteLine(line);
 Clock.Now=0;Clock.Q.Clear();Clock.Seq=0;var detached=new Manager{Name="detached_zero"};detached.ExecuteEmoteSet(Set((3u,0f,0f),(10u,0f,0f)),null);if(detached.IsBusy)throw new Exception("outer still busy");detached.ExecuteEmoteSet(Set((1u,0f,0f)),null);Clock.Run(0);foreach(var line in detached.Trace)Console.WriteLine(line);
 }
}
