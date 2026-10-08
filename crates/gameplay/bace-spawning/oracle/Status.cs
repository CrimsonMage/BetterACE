public enum GeneratorTimeType {Undefined,RealTime,Defined,Event,Night,Day}
public static class Time {public static int Now;public static double GetUnixTime()=>Now;}
public static class Timers {public class InGameTime {public bool IsDay;}public static InGameTime CurrentInGameTime=new();}
public static class EventManager {public static bool Available=true,Enabled=true,Started=true;public static bool IsEventAvailable(string n)=>Available;public static bool IsEventEnabled(string n)=>Enabled;public static bool IsEventStarted(string n,WorldObject o,object x)=>Started;}
public partial class WorldObject {public GeneratorTimeType GeneratorTimeType;public bool GeneratorDisabled,GeneratorEnteredWorld,FirstEnterWorldDone;public int GeneratorStartTime,GeneratorEndTime;public string GeneratorEvent="event";public void StartGenerator(){}public void DisableGenerator(){}}
public static class StatusCases {
 public static void Run(){foreach(var mode in new[]{GeneratorTimeType.Day,GeneratorTimeType.Night,GeneratorTimeType.RealTime,GeneratorTimeType.Event,GeneratorTimeType.Defined}){
  var g=new WorldObject(){GeneratorTimeType=mode,GeneratorStartTime=10,GeneratorEndTime=20};
  for(int i=0;i<7;i++){Time.Now=new[]{0,10,20,21,19,21,21}[i];Timers.CurrentInGameTime.IsDay=new[]{true,false,true,false,false,true,true}[i];EventManager.Available=i!=4;EventManager.Enabled=i!=1;EventManager.Started=i!=2&&i!=5;g.Generator_Update();Console.WriteLine($"{mode}|{i}|{g.GeneratorDisabled}");}
 }}
}
