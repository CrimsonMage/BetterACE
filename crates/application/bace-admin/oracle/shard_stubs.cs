// Harness boundaries only. Selected ACE methods are injected unchanged by run_shard.py.
using System.Text.Json;
using ACE.Entity.Enum;
using ACE.Server.Network;
using ACE.Server.Managers;
using AdminShardCommands=ACE.Server.Command.Handlers.AdminShardCommands;
using ACE.Common.Extensions;
public static class Capture {
 public static List<object> Events=new(); public static List<string> Boots=new();
 public static void Add(string kind,string text)=>Events.Add(new {kind,text});
 public static System.DateTime Now=new(2024,10,9,0,0,0,System.DateTimeKind.Utc);
}
namespace log4net {public interface ILog {void Info(object s);} public class Logger:ILog {public void Info(object s)=>Capture.Add("Log",s.ToString());} public static class LogManager {public static ILog GetLogger(Type t)=>new Logger();}}
namespace ACE.Entity.Enum {public enum ChatMessageType{Broadcast,WorldBroadcast} public enum SessionTerminationReason{WorldClosed}}
namespace ACE.Server.Network.GameMessages.Messages {
 public class GameMessageSystemChat {public string Text;public GameMessageSystemChat(string text,ChatMessageType c){Text=text;}}
 public class GameMessageBootAccount {public string Text;public GameMessageBootAccount(string t){Text=t;}}
}
namespace ACE.Server.Network {
 public class Player {public string Name;public Session Session; public Player(string name){Name=name;}}
 public class Session {
  public Player Player; public AccessLevel AccessLevel;
  public Session(string n,AccessLevel access=AccessLevel.Admin){Player=new Player(n);Player.Session=this;AccessLevel=access;}
  public void WorldBroadcast(string s)=>Capture.Add("Broadcast",s);
  public void Terminate(SessionTerminationReason reason,ACE.Server.Network.GameMessages.Messages.GameMessageBootAccount message,object error,string description)=>Capture.Boots.Add(Player.Name+"|"+message.Text+"|"+description);
 }
}
namespace ACE.Server.Command {
 public static class CommandHandlerHelper {public static void WriteOutputInfo(Session s,string t,ChatMessageType c)=>Capture.Add("Reply"+c,t);}
}
namespace ACE.Server.Command.Handlers {public static class DateTime {public static System.DateTime Now=>Capture.Now;public static System.DateTime UtcNow=>Capture.Now;}}
namespace ACE.Server.Managers {
 using ACE.Server.Network.GameMessages.Messages;
 public static class ServerManager {
  static log4net.ILog log=log4net.LogManager.GetLogger(typeof(ServerManager));
  public static bool ShutdownInitiated;public static uint ShutdownInterval;
  public static System.DateTime ShutdownTime=System.DateTime.MinValue;
  /*SET*/
  /*CANCEL*/
  public static void BeginShutdown(){ShutdownInitiated=true;ShutdownTime=Capture.Now.AddSeconds(ShutdownInterval);}
 }
 public static class WorldManager {
  public enum WorldStatusState{Closed,Open}public static WorldStatusState WorldStatus;
  /*OPEN*/
  /*CLOSE*/
 }
 public static class PlayerManager {
  public static List<Player> Players=new();
  public static IEnumerable<Player> GetAllOnline()=>Players;
  public static void BroadcastToAuditChannel(Player player,string text)=>Capture.Add("Audit",text);
  public static void BroadcastToAll(GameMessageSystemChat chat)=>Capture.Add("Broadcast",chat.Text);
  /*BOOT*/
 }
}
public readonly struct OracleDateTime {
 public readonly long Millis; public OracleDateTime(long millis){Millis=millis;}
 public static OracleDateTime UtcNow=>new(10000000);
 public static TimeSpan operator -(OracleDateTime a,OracleDateTime b)=>TimeSpan.FromMilliseconds(a.Millis-b.Millis);
}
public static class Program {
 public static void Main(){
 var cases=new List<object>();
 foreach(var row in JsonDocument.Parse(File.ReadAllText("input.json")).RootElement.EnumerateArray()){
  Capture.Events.Clear();Capture.Boots.Clear();
  ServerManager.ShutdownInterval=row.GetProperty("interval").GetUInt32();ServerManager.ShutdownInitiated=row.GetProperty("pending").GetBoolean();ServerManager.ShutdownTime=ServerManager.ShutdownInitiated?Capture.Now.AddSeconds(60):System.DateTime.MinValue;
  WorldManager.WorldStatus=row.GetProperty("opened").GetBoolean()?WorldManager.WorldStatusState.Open:WorldManager.WorldStatusState.Closed;
  PlayerManager.Players=Enum.GetValues<AccessLevel>().Select(a=>new Session(a.ToString(),a).Player).ToList();
  var args=row.GetProperty("args").EnumerateArray().Select(x=>x.GetString()).ToArray();
  var name=row.GetProperty("name").GetString();var session=name==null?null:new Session(name);
  switch(row.GetProperty("command").GetString()){
   case "shutdown":AdminShardCommands.ShutdownServer(session,args);break;
   case "stop-now":AdminShardCommands.ShutdownServerNow(session,args);break;
   case "cancel-shutdown":AdminShardCommands.HandleCancelShutdown(session,args);break;
   case "set-shutdown-interval":AdminShardCommands.HandleSetShutdownInterval(session,args);break;
   case "world":AdminShardCommands.HandleHelp(session,args);break;
  }
  cases.Add(new {input=row.Clone(),events=Capture.Events.ToArray(),boots=Capture.Boots.ToArray(),interval=ServerManager.ShutdownInterval,pending=ServerManager.ShutdownInitiated,opened=WorldManager.WorldStatus==WorldManager.WorldStatusState.Open});
 }
 var notices=new List<object>();PlayerManager.Players=new(){new Session("Alyssa").Player};
 foreach(var row in JsonDocument.Parse(File.ReadAllText("notices.json")).RootElement.EnumerateArray()){
  Capture.Events.Clear();var last=10000000-row.GetProperty("elapsed").GetInt64();var deadline=10000000+row.GetProperty("remaining").GetInt64();
  var result=NotifyOracle.Run(last,deadline);notices.Add(new {input=row.Clone(),events=Capture.Events.ToArray(),updated=result!=last});
 }
 var dates=new List<object>();foreach(var row in JsonDocument.Parse(File.ReadAllText("dates.json")).RootElement.EnumerateArray()){var unix=row.GetInt64();dates.Add(new {unix,text=DateTimeOffset.FromUnixTimeMilliseconds(unix).UtcDateTime.ToCommonString()});}
 Console.WriteLine(JsonSerializer.Serialize(new {cases,notices,dates}));
 }
}
