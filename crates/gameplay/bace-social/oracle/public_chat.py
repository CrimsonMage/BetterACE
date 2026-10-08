#!/usr/bin/env python3
"""Execute unmodified public-channel gate blocks and HandleChatReject from pinned ACE."""
from pathlib import Path
import subprocess,tempfile,hashlib,base64
ROOT=Path(__file__).resolve().parents[4];SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source';f=SRC/'ACE.Server/Network/Handlers/TurbineChatHandler.cs';text=f.read_text()
def block(start):
 a=text.index(start);i=text.index('{',a)+1;d=1
 while d:d+=(text[i]=='{')-(text[i]=='}');i+=1
 return text[text.index('{',a)+1:i-1]
def method(sig):return sig+'{'+block(sig)+'}'
public=block('else // Channel must be one of the channels available to all players')
# Explicit deterministic clock replaces only source wall-clock acquisition.
public=public.replace('DateTime.UtcNow','Clock.Now')
reject=method('private static void HandleChatReject(Session session, uint contextId, ChatType chatType, GameMessageTurbineChat gameMessageTurbineChat, string rejectReason)')
h='''using System;using System.Collections.Generic;using ACE.Entity.Enum;using ACE.Server.Entity;
enum CharacterOption{ListenToGeneralChat,ListenToTradeChat,ListenToLFGChat,ListenToRoleplayChat}
class Clock{public static DateTime Now=new DateTime(2020,1,2,0,0,0,DateTimeKind.Utc);}
class Property<T>{public T Item;public Property(T v){Item=v;}}class PropertyManager{public static Dictionary<string,bool>B=new();public static Dictionary<string,long>L=new();public static Property<bool>GetBool(string k)=>new(B.GetValueOrDefault(k));public static Property<long>GetLong(string k)=>new(L.GetValueOrDefault(k));}
class Account{public DateTime CreateTime;}class Player{public bool IsOlthoiPlayer;public bool Account15Days;public Account Account=new();public int Age,Level;public bool GetCharacterOption(CharacterOption o)=>true;public SquelchManager SquelchManager=new();public Session Session;}
class SquelchManager{public Squelches Squelches=new();}class Squelches{public bool Contains(Player p,ChatMessageType t)=>false;}
class Session{public Player Player;public Network Network=new();}class Network{public void EnqueueSend(object v){if(v is GameEventCommunicationTransientString t)Program.Notice=t.Text;if(v is GameMessageTurbineChat t2){if(t2.Kind==ChatNetworkBlobType.NETBLOB_RESPONSE_BINARY)Program.Ack++;else Program.Echo++;}}}
class PlayerManager{public static Player Recipient=new(){Session=new()};public static IEnumerable<Player>GetAllOnline(){yield return Recipient;}}
class GameMessageTurbineChat{public ChatNetworkBlobType Kind;public GameMessageTurbineChat(ChatNetworkBlobType kind,ChatNetworkBlobDispatchType d,uint context,object n,object t,uint s,ChatType type){Kind=kind;}}
class GameEventCommunicationTransientString{public string Text;public GameEventCommunicationTransientString(Session s,string t){Text=t;}}
class GameMessageSystemChat{public GameMessageSystemChat(string t,ChatMessageType k){}}
class Program{public static int Echo,Ack;public static string Notice="";static void Main(){for(int config=0;config<9;config++)for(int evidence=0;evidence<4;evidence++)for(uint channel=2;channel<=5;channel++){PropertyManager.B.Clear();PropertyManager.L.Clear();PropertyManager.B["chat_inform_reject"]=true;if(config==1)PropertyManager.B["chat_echo_only"]=true;if(config==2)PropertyManager.B["chat_requires_account_15days"]=true;if(config==3)PropertyManager.L["chat_requires_account_time_seconds"]=100;if(config==4)PropertyManager.L["chat_requires_player_age"]=100;if(config==5)PropertyManager.L["chat_requires_player_level"]=10;if(config==6)PropertyManager.B["chat_disable_general"]=true;if(config==7){PropertyManager.B["chat_disable_trade"]=true;PropertyManager.B["chat_echo_reject"]=true;}if(config==8){PropertyManager.L["chat_requires_player_age"]=100;PropertyManager.B["chat_inform_reject"]=false;}var p=new Player{Account15Days=evidence>=1,Age=evidence>=2?100:99,Level=evidence>=3?10:9,Account=new Account{CreateTime=Clock.Now.AddSeconds(evidence>=1?-100:-99)}};var s=new Session{Player=p};p.Session=s;Echo=Ack=0;Notice="";Run(s,channel);Console.WriteLine($"{config},{evidence},{channel},{Echo},{Ack},{Convert.ToBase64String(System.Text.Encoding.UTF8.GetBytes(Notice))}");}}
static void Run(Session session,uint channelID){uint adjustedChannelID=channelID,contextId=1;var chatType=(ChatType)channelID;var adjustedchatType=chatType;var gameMessageTurbineChat=new GameMessageTurbineChat(ChatNetworkBlobType.NETBLOB_EVENT_BINARY,ChatNetworkBlobDispatchType.ASYNCMETHOD_SENDTOROOMBYNAME,channelID,null,null,0,chatType);PUBLIC}
REJECT}
'''.replace('PUBLIC',public).replace('REJECT',reject)
files=[SRC/('ACE.Entity/Enum/'+n+'.cs')for n in ['ChatMessageType','SquelchMask','ChatType','ChatNetworkBlobDispatchType','ChatNetworkBlobType']]+[SRC/'ACE.Server/Entity/TurbineChatChannel.cs']
with tempfile.TemporaryDirectory(prefix='public-chat-') as tmp:
 p=Path(tmp);(p/'Program.cs').write_text(h)
 for i,v in enumerate(files):(p/f'source{i}.cs').write_bytes(v.read_bytes())
 (p/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 subprocess.run(['/tmp/bace-crafting-dotnet/dotnet','build','--nologo','-o',str(p/'build')],cwd=p,check=True)
 out=subprocess.check_output(['/tmp/bace-crafting-dotnet/dotnet',str(p/'build/oracle.dll')],text=True)
 out=''.join(','.join(line.split(',')[:5])+','+base64.b64decode(line.split(',')[5]).decode()+'\n' for line in out.splitlines())
 Path(__file__).parents[1].joinpath('tests/fixtures/public_chat.csv').write_text('# '+str(f.relative_to(SRC))+' '+hashlib.sha256(f.read_bytes()).hexdigest()+'\n'+out)
