using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Collections.Generic;
using ACE.Entity;
using ACE.Entity.Enum;
using ACE.Database.Models.Shard;
using ACE.Server.Network;
using ACE.Server.Network.GameAction;
using ACE.Server.Network.GameAction.Actions;
using ACE.Server.Network.GameMessages;
using ACE.Server.Network.GameMessages.Messages;
using ACE.Server.Network.GameEvent.Events;
using ACE.Server.Network.Structure;
using ACE.Server.WorldObjects;
using ACE.Server.Managers;

static class SocialHarness {
    static Session Fresh()=>new(){Account="synthetic-account",Player=new HarnessPlayer{Guid=new ObjectGuid(0x50000001)},GameEventSequence=42};
    static object Message(GameMessage message)=>new{bytes=Convert.ToHexString(message.Data.ToArray()),group=(uint)message.Group};
    static void ClientString(BinaryWriter writer,string value){writer.Write((ushort)value.Length);writer.Write(Encoding.UTF8.GetBytes(value));writer.Write(new byte[(4-(value.Length+2)%4)%4]);}
    static void PackedUnicode(BinaryWriter writer,string value){if(value.Length<128)writer.Write((byte)value.Length);else{writer.Write((byte)(0x80|(value.Length>>8)));writer.Write((byte)value.Length);}writer.Write(Encoding.Unicode.GetBytes(value));}
    public static object Run(){
        var turbineEvents=new[]{("Élodie","Hello🙂"),("",""),("Sender",new string('x',127)),("Sender",new string('x',128)),("Sender",new string('x',255)),("Sender",new string('x',256)),(new string('n',128),"abc")}.Select(input=>new{sender_name=input.Item1,text=input.Item2,supported=input.Item1.Length<128&&input.Item2.Length<256,message=Message(new GameMessageTurbineChat(ChatNetworkBlobType.NETBLOB_EVENT_BINARY,ChatNetworkBlobDispatchType.ASYNCMETHOD_SENDTOROOMBYNAME,2,input.Item1,input.Item2,0x50000001,ChatType.General))}).ToArray();
        var turbineResponses=new uint[]{0,42,uint.MaxValue}.Select(context=>new{context_id=context,message=Message(new GameMessageTurbineChat(ChatNetworkBlobType.NETBLOB_RESPONSE_BINARY,ChatNetworkBlobDispatchType.ASYNCMETHOD_SENDTOROOMBYNAME,context,null,null,0,ChatType.General))}).ToArray();
        var turbineRequests=new List<object>();
        foreach(var length in new[]{0,3,127,128,255,256,1024})foreach(uint dispatch in new uint[]{1,2,9}){
            var text=length==3?"é🙂":new string('x',length);
            using var ms=new MemoryStream();using var writer=new BinaryWriter(ms);writer.Write((uint)GameMessageOpcode.TurbineChat);
            foreach(uint word in new uint[]{uint.MaxValue,3,dispatch,1,0,0,0,0,0})writer.Write(word);
            writer.Write(42u);writer.Write(2u);writer.Write(2u);writer.Write(2u);PackedUnicode(writer,text);
            foreach(uint word in new uint[]{12,0xdeadbeef,0,2})writer.Write(word);writer.Write(new byte[]{0xa5,0x5a});
            var bytes=ms.ToArray();var input=new ClientMessage(bytes);var decoded=ExtractedSocialReaders.Turbine(input);
            turbineRequests.Add(new{bytes=Convert.ToHexString(bytes),decoded,consumed=input.Data.Position,trailing_bytes=input.Data.Length-input.Data.Position,valid_utf16=true});
        }
        {
            using var ms=new MemoryStream();using var writer=new BinaryWriter(ms);writer.Write((uint)GameMessageOpcode.TurbineChat);
            foreach(uint word in new uint[]{0,3,2,1,0,0,0,0,0,42,2,2,2})writer.Write(word);writer.Write((byte)1);writer.Write((ushort)0xd800);
            foreach(uint word in new uint[]{12,0xdeadbeef,0,2})writer.Write(word);
            var bytes=ms.ToArray();var input=new ClientMessage(bytes);var decoded=ExtractedSocialReaders.Turbine(input);
            turbineRequests.Add(new{bytes=Convert.ToHexString(bytes),decoded,consumed=input.Data.Position,trailing_bytes=0,valid_utf16=false});
        }
        var actions=new[]{GameActionType.Talk,GameActionType.Tell,GameActionType.TalkDirect,GameActionType.ChatChannel,GameActionType.Emote,GameActionType.SoulEmote,GameActionType.SetAfkMode,GameActionType.SetAfkMessage,GameActionType.AddFriend,GameActionType.RemoveFriend,GameActionType.RemoveAllFriends,GameActionType.AddChannel,GameActionType.RemoveChannel,GameActionType.ModifyGlobalSquelch,GameActionType.ModifyCharacterSquelch,GameActionType.ModifyAccountSquelch}.Select(action=>{
            using var ms=new MemoryStream();using var writer=new BinaryWriter(ms);writer.Write((uint)GameMessageOpcode.GameAction);writer.Write(42u);writer.Write((uint)action);
            switch(action){
                case GameActionType.Talk:case GameActionType.Emote:case GameActionType.SoulEmote:case GameActionType.SetAfkMessage:ClientString(writer,"Hello🙂");break;
                case GameActionType.Tell:ClientString(writer,"Hello🙂");ClientString(writer,"  Élodie  ");break;
                case GameActionType.TalkDirect:ClientString(writer,"Hello🙂");writer.Write(0x50000002u);break;
                case GameActionType.ChatChannel:writer.Write(2u);ClientString(writer,"Hello🙂");break;
                case GameActionType.SetAfkMode:writer.Write(2u);break;
                case GameActionType.AddFriend:ClientString(writer,"  Élodie  ");break;
                case GameActionType.RemoveFriend:writer.Write(0x50000002u);break;
                case GameActionType.AddChannel:case GameActionType.RemoveChannel:writer.Write(2u);break;
                case GameActionType.ModifyGlobalSquelch:writer.Write(2u);writer.Write(3u);break;
                case GameActionType.ModifyCharacterSquelch:writer.Write(2u);writer.Write(0x50000002u);ClientString(writer,"Élodie");writer.Write(3u);break;
                case GameActionType.ModifyAccountSquelch:writer.Write(2u);ClientString(writer,"Élodie");break;
            }
            writer.Write(new byte[]{0xa5,0x5a});var bytes=ms.ToArray();var input=new ClientMessage(bytes);input.Payload.ReadUInt32();input.Payload.ReadUInt32();var session=Fresh();
            object captured=null;
            switch(action){
                case GameActionType.Talk:captured=ExtractedSocialReaders.Talk(input);break;
                case GameActionType.Tell:captured=ExtractedSocialReaders.Tell(input);break;
                case GameActionType.TalkDirect:captured=ExtractedSocialReaders.TalkDirect(input);break;
                case GameActionType.ChatChannel:captured=ExtractedSocialReaders.ChatChannel(input);break;
                case GameActionType.AddChannel:captured=ExtractedSocialReaders.AddChannel(input);break;
                case GameActionType.RemoveChannel:captured=ExtractedSocialReaders.RemoveChannel(input);break;
                case GameActionType.Emote:GameActionEmote.Handle(input,session);break;
                case GameActionType.SoulEmote:GameActionSoulEmote.Handle(input,session);break;
                case GameActionType.SetAfkMode:GameActionSetAFKMode.Handle(input,session);break;
                case GameActionType.SetAfkMessage:GameActionSetAFKMessage.Handle(input,session);break;
                case GameActionType.AddFriend:GameActionAddFriend.Handle(input,session);break;
                case GameActionType.RemoveFriend:GameActionRemoveFriend.Handle(input,session);break;
                case GameActionType.RemoveAllFriends:GameActionRemoveAllFriends.Handle(input,session);break;
                case GameActionType.ModifyGlobalSquelch:GameActionModifyGlobalSquelch.Handle(input,session);break;
                case GameActionType.ModifyCharacterSquelch:GameActionModifyCharacterSquelch.Handle(input,session);break;
                case GameActionType.ModifyAccountSquelch:GameActionModifyAccountSquelch.Handle(input,session);break;
            }
            return new{bytes=Convert.ToHexString(bytes),action=(uint)action,captured=captured??session.Player.SocialCapture,consumed=input.Data.Position,trailing_bytes=input.Data.Length-input.Data.Position};
        }).ToArray();
        PlayerManager.Players.Clear();PlayerManager.Online.Clear();
        PlayerManager.Players.Add(0x50000002,new Player{Guid=new ObjectGuid(0x50000002),Name="Élodie",ChannelsActive=Channel.Admin});
        PlayerManager.Players.Add(0x50000003,new Player{Guid=new ObjectGuid(0x50000003),Name="Hidden friend",AppearOffline=true,ChannelsActive=Channel.Admin});
        PlayerManager.Players.Add(0x50000004,new Player{Guid=new ObjectGuid(0x50000004),Name="Offline"});
        PlayerManager.Online.UnionWith(new uint[]{0x50000002,0x50000003});
        var full=Fresh();full.Player.Character.Friends.AddRange(new[]{new CharacterPropertiesFriendList{FriendId=0x50000003},new CharacterPropertiesFriendList{FriendId=0x50000002},new CharacterPropertiesFriendList{FriendId=0x50000004},new CharacterPropertiesFriendList{FriendId=0x500000ff}});
        var events=new Dictionary<string,object>{
            ["tell"]=Message(new GameEventTell(Fresh(),"Hello €","Élodie",0x50000002,0x50000001,(ChatMessageType)3)),
            ["channel_broadcast"]=Message(new GameEventChannelBroadcast(Fresh(),Channel.Admin,"Élodie","Hello €")),
            ["transient"]=Message(new GameEventCommunicationTransientString(Fresh(),"Hello €")),
            ["turbine_channels"]=Message(new GameEventSetTurbineChatChannels(Fresh(),0x12345678,0x23456789)),
            ["friends_full"]=Message(new GameEventFriendsListUpdate(full)),["friends_empty"]=Message(new GameEventFriendsListUpdate(Fresh())),
            ["friend_added"]=Message(new GameEventFriendsListUpdate(Fresh(),GameEventFriendsListUpdate.FriendsUpdateTypeFlag.FriendAdded,new CharacterPropertiesFriendList{FriendId=0x50000004},true,true)),
            ["friend_removed"]=Message(new GameEventFriendsListUpdate(Fresh(),GameEventFriendsListUpdate.FriendsUpdateTypeFlag.FriendRemoved,new CharacterPropertiesFriendList{FriendId=0x500000ff})),
            ["friend_status"]=Message(new GameEventFriendsListUpdate(Fresh(),GameEventFriendsListUpdate.FriendsUpdateTypeFlag.FriendStatusChanged,new CharacterPropertiesFriendList{FriendId=0x50000003},true,true)),
            ["channel_list"]=Message(new GameEventChannelList(Fresh(),Channel.Admin)),
        };
        foreach(var role in new[]{"none","admin","sentinel","advocate","overlap"}){
            var session=Fresh();session.Player.IsAdmin=role=="admin"||role=="overlap";session.Player.IsSentinel=role=="sentinel"||role=="overlap";session.Player.IsAdvocate=role=="advocate"||role=="overlap";
            events["channel_index_"+role]=Message(new GameEventChannelIndex(session));
        }
        var db=new SquelchDB();
        db.CharactersPlus.Add(0x50000021,new SquelchInfo((SquelchMask)0x12345678,"Élodie",false));
        db.CharactersPlus.Add(0x50000020+32,new SquelchInfo(new List<SquelchMask>{(SquelchMask)1,(SquelchMask)2},"Account alias",true));
        db.CharactersPlus.Add(0x50000020,new SquelchInfo(new List<SquelchMask>(),"Offline",false));
        db.Globals=new SquelchInfo(new List<SquelchMask>{(SquelchMask)64},"",false);
        events["squelch"]=Message(new GameEventSetSquelchDB(Fresh(),db));
        events["squelch_empty"]=Message(new GameEventSetSquelchDB(Fresh(),new SquelchDB()));
        return new{turbine_events=turbineEvents,turbine_responses=turbineResponses,turbine_requests=turbineRequests,actions,events};
    }
}
namespace ACE.Server.WorldObjects {
    public partial class WorldObject {public CreatureType CreatureType;public HarnessAccount Account=new();}
    public sealed partial class HarnessCharacter {
        public List<CharacterPropertiesFriendList> Friends=new();
        public List<CharacterPropertiesFriendList> GetFriends(object gate)=>Friends;
    }
    public partial class Player {
        public Session Session;public bool IsAdmin,IsArch,IsSentinel,IsAdvocate;
        public Channel? ChannelsActive;
        public bool AppearOffline;public bool GetAppearOffline()=>AppearOffline;
        public HarnessCharacter Character=new();public object CharacterDatabaseLock=new();
    }
}
namespace ACE.Server.Managers {
    public static partial class PlayerManager {
        public static Dictionary<uint,WorldObject> Players=new();public static HashSet<uint> Online=new();
        public static WorldObject FindByGuid(uint guid,out bool online) {online=Online.Contains(guid);return Players.GetValueOrDefault(guid);}
        public static Player GetOnlinePlayer(uint guid)=>Online.Contains(guid)?Players.GetValueOrDefault(guid) as Player:null;
        public static IEnumerable<Player> GetAllOnline()=>Players.Where(kvp=>Online.Contains(kvp.Key)).Select(kvp=>kvp.Value).OfType<Player>();
    }
}
namespace ACE.Server.Network {
    public sealed partial class HarnessPlayer {
        public object SocialCapture;
        public HarnessSquelchManager SquelchManager;
        public HarnessPlayer(){SquelchManager=new(this);}
        public void HandleActionAddFriend(string name){SocialCapture=new{name};}
        public void HandleActionRemoveFriend(uint id){SocialCapture=new{object_id=id};}
        public void HandleActionRemoveAllFriends(){SocialCapture=new{empty=true};}
        public void HandleActionEmote(string text){SocialCapture=new{text};}
        public void HandleActionSoulEmote(string text){SocialCapture=new{text};}
        public void HandleActionSetAFKMode(bool enabled){SocialCapture=new{enabled};}
        public void HandleActionSetAFKMessage(string text){SocialCapture=new{text};}
    }
    public sealed class HarnessSquelchManager {
        readonly HarnessPlayer player;public HarnessSquelchManager(HarnessPlayer p){player=p;}
        public void HandleActionModifyGlobalSquelch(bool enabled,ChatMessageType type){player.SocialCapture=new{enabled,message_type=(uint)type};}
        public void HandleActionModifyCharacterSquelch(bool enabled,uint id,string name,ChatMessageType type){player.SocialCapture=new{enabled,object_id=id,name,message_type=(uint)type};}
        public void HandleActionModifyAccountSquelch(bool enabled,string name){player.SocialCapture=new{enabled,name};}
    }
}
namespace ACE.Server.Network.Structure {
    public sealed class SquelchDB {public Dictionary<uint,SquelchInfo> CharactersPlus=new();public SquelchInfo Globals=new();}
}
