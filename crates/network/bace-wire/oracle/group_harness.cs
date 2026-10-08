using System;using System.IO;using System.Text;using System.Collections.Generic;using System.Numerics;
using ACE.Entity;using ACE.Entity.Enum;using ACE.Server.Entity;using ACE.Server.Network;using ACE.Server.Network.Structure;using ACE.Server.Network.GameEvent;using ACE.Server.Network.GameEvent.Events;using ACE.Server.WorldObjects;using ACE.Common.Extensions;
class Program {
 static void Main(){Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);INPUT_CASES
  var s=new Session();var f=new Fellowship{FellowshipName="Rune",FellowshipLeaderGuid=17,ShareXP=true,EvenShare=false,Open=true,IsLocked=true};s.Player.Fellowship=f;
  foreach(uint id in new[]{17u,32u,1u})f.Members[id]=new Player{Guid=new(id),Name="Fellow"+id,Level=(int)id,Health=new(100,90),Stamina=new(200,80),Mana=new(300,70)};
  f.DepartedMembers[34]=123;f.DepartedMembers[1]=456;f.FellowshipLocks["Rune"]=new FellowshipLockData(789);
  Emit("fellowship",new GameEventFellowshipFullUpdate(s));
  Emit("fellow",new GameEventFellowshipUpdateFellow(s,f.Members[17],true,FellowUpdateType.Full));
  Emit("quit",new GameEventFellowshipQuit(s,17));Emit("dismiss",new GameEventFellowshipDismiss(s,f.Members[17]));Emit("disband",new GameEventFellowshipDisband(s));Emit("fellowdone",new GameEventFellowshipFellowUpdateDone(s));
  Emit("confirm",new GameEventConfirmationRequest(s,ConfirmationType.Fellowship,77,"Rune"));Emit("confirmdone",new GameEventConfirmationDone(s,ConfirmationType.Fellowship,77));
  Emit("allegiancedone",new GameEventAllegianceAllegianceUpdateDone(s));Emit("empty",new GameEventAllegianceUpdate(s,null,null));
  var monarch=Node(1,5);var self=Node(17,2);var child=Node(32,1);self.Patron=monarch;self.Monarch=monarch;child.Patron=self;child.Monarch=monarch;monarch.Monarch=monarch;monarch.IsMonarch=true;monarch.TotalFollowers=2;self.TotalFollowers=1;self.Vassals[32]=child;
  var a=new Allegiance{Monarch=monarch,AllegianceName="Rune",Biota=new(){Id=700},Sanctuary=new(){Cell=0x12340001,Pos=new(10,20,42),Rotation=Quaternion.Identity}};
  Emit("allegiance",new GameEventAllegianceUpdate(s,a,self));Emit("info",new GameEventAllegianceInfoResponse(s,17,new AllegianceProfile(a,self)));
 }
 static AllegianceNode Node(uint id,uint rank){var p=new Player{Guid=new(id),Name="Member"+id,Level=(int)id,Gender=1,Heritage=1,AllegianceXPCached=100+id,AllegianceXPGenerated=200+id,ExistedBeforeAllegianceXpChanges=true};ACE.Server.Managers.PlayerManager.Players[id]=p;return new(){Player=p,Rank=rank};}
 static void Emit(string name,GameEventMessage message)=>Console.WriteLine("output,"+name+","+Convert.ToHexString(message.Data.ToArray()));
}
namespace ACE.Entity {public struct ObjectGuid {public uint Full;public ObjectGuid(uint id){Full=id;}}public class Position {public uint Cell;public Vector3 Pos;public Quaternion Rotation=Quaternion.Identity;}}
namespace ACE.Entity.Enum {public enum WeenieError{None}public enum Skill{None}}
namespace ACE.Entity.Enum.Properties {public enum PropertyInt{}public enum PropertyInt64{}public enum PropertyBool{}public enum PropertyFloat{}public enum PropertyString{}public enum PropertyDataId{}public enum PropertyInstanceId{}}
namespace ACE.Database.Models.Shard {public class CharacterPropertiesFillCompBook {public uint SpellComponentId,QuantityToRebuy;}}
namespace ACE.Server.Network {public class ClientMessage {public BinaryReader Payload;} public class Session {public Player Player=new(){Guid=new(99)};}public enum GameMessageGroup {UIQueue}}
namespace ACE.Server.Network.GameEvent {public class GameEventMessage {public MemoryStream Data=new();public BinaryWriter Writer;public GameEventMessage(GameEventType op,GameMessageGroup group,Session session,int capacity=0){Writer=new(Data);Writer.Write(0xf7b0u);Writer.Write(session.Player.Guid.Full);Writer.Write(7u);Writer.Write((uint)op);}}}
namespace ACE.Server.WorldObjects {public class Vital {public uint MaxValue,Current;public Vital(uint max,uint current){MaxValue=max;Current=current;}}public class Player {public ObjectGuid Guid;public string Name;public int? Level;public int Gender,Heritage;public ulong AllegianceXPCached,AllegianceXPGenerated;public bool ExistedBeforeAllegianceXpChanges;public Fellowship Fellowship;public Vital Health,Stamina,Mana;public int GetCurrentLoyalty()=>80;public int GetCurrentLeadership()=>90;}}
namespace ACE.Server.Entity {public class Fellowship {public string FellowshipName;public uint FellowshipLeaderGuid;public bool ShareXP,EvenShare,Open,IsLocked;public Dictionary<uint,Player> Members=new();public Dictionary<uint,int> DepartedMembers=new();public Dictionary<string,FellowshipLockData> FellowshipLocks=new();public Dictionary<uint,Player> GetFellowshipMembers()=>Members;}public class AllegianceNode {public Player Player;public ObjectGuid PlayerGuid=>Player.Guid;public AllegianceNode Monarch,Patron;public bool IsMonarch;public uint Rank;public int TotalFollowers;public int TotalVassals=>Vassals.Count;public Dictionary<uint,AllegianceNode> Vassals=new();}public class Biota {public uint Id;}public class Allegiance {public AllegianceNode Monarch;public string AllegianceName;public Position Sanctuary;public Biota Biota;}}
namespace ACE.Server.Managers {public static class PlayerManager {public static Dictionary<uint,Player> Players=new();public static Player FindByGuid(ObjectGuid id,out bool online){online=id.Full!=32;return Players[id.Full];}}}
