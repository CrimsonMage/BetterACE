#!/usr/bin/env python3
"""Compile pinned legacy Fellow and CoVassals action blocks verbatim."""
from pathlib import Path
import hashlib,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[4];SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
f=SRC/'ACE.Server/Network/GameAction/Actions/GameActionChatChannel.cs';s=f.read_text()
def block(name):
 a=s.index('{',s.index('case Channel.'+name+':'));i=a+1;depth=1
 while depth:depth+=(s[i]=='{')-(s[i]=='}');i+=1
 return s[a+1:i-1].replace('break;','return;')
h='''using System;using System.Collections.Generic;using System.Linq;
enum Channel:uint{Fellow=2048,Patron=8192,CoVassals=0x1000000}enum ChatMessageType{Fellowship,Allegiance}enum WeenieError{YouDoNotBelongToAFellowship,YouAreNotInAllegiance,YouCantUseThatChannel}
class GameEventWeenieError{public GameEventWeenieError(Session s,WeenieError e){}}
class GameEventChannelBroadcast{public Session Recipient;public Channel Channel;public string Name;public GameEventChannelBroadcast(Session s,Channel c,string n,string m){Recipient=s;Channel=c;Name=n;}}
class Network{public void EnqueueSend(object e){if(e is GameEventChannelBroadcast c)Program.output.Add($"{c.Recipient.Player.Guid.Full}:{(uint)c.Channel}:{(c.Name==""?0:1)}");}}
class Session{public Player Player;public Network Network=new();}class GuidValue{public uint Full;}class Squelches{public bool blocked;public bool Contains(Player p,ChatMessageType t)=>blocked;}class SquelchManager{public Squelches Squelches=new();}
class Node{public uint PlayerGuid;public Node Patron;public Dictionary<uint,int> Vassals=new();}
class Fellowship{public Dictionary<uint,Player> GetFellowshipMembers()=>PlayerManager.online;}
class Player{public GuidValue Guid=new();public string Name="Sender";public Session Session;public bool HasAllegiance=true;public uint? PatronId=2;public Node AllegianceNode;public Fellowship Fellowship=new();public SquelchManager SquelchManager=new();}
class PlayerManager{public static Dictionary<uint,Player>online=new();public static Player GetOnlinePlayer(uint id)=>online.GetValueOrDefault(id);}
class Program{public static List<string>output=new();static void Main(){foreach(var kind in new[]{"F","C"})for(int mask=0;mask<8;mask++)for(int online=1;online<8;online+=2){PlayerManager.online.Clear();output.Clear();var patron=new Node{PlayerGuid=2,Vassals=new(){{1,0},{3,0}}};for(uint id=1;id<=3;id++){var p=new Player{Guid=new(){Full=id},AllegianceNode=new(){Patron=patron}};p.Session=new(){Player=p};p.SquelchManager.Squelches.blocked=(mask&(1<<(int)(id-1)))!=0;if((online&(1<<(int)(id-1)))!=0)PlayerManager.online[id]=p;}Run(PlayerManager.online[1].Session,kind);Console.WriteLine($"{kind},{mask},{online},{string.Join(';',output)}");}}
static void Run(Session session,string kind){var message="hello";var groupChatType=kind=="F"?Channel.Fellow:Channel.CoVassals;if(kind=="F"){FELLOW}else{COVASSALS}}}
'''.replace('FELLOW',block('Fellow')).replace('COVASSALS',block('CoVassals'))
with tempfile.TemporaryDirectory(prefix='legacy-chat-') as tmp:
 p=Path(tmp);(p/'Program.cs').write_text(h);(p/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><WarningLevel>0</WarningLevel></PropertyGroup></Project>')
 subprocess.run(['/tmp/bace-crafting-dotnet/dotnet','build','--nologo','-o',str(p/'build')],cwd=p,check=True)
 out=subprocess.check_output(['/tmp/bace-crafting-dotnet/dotnet',str(p/'build/oracle.dll')],text=True)
 (Path(__file__).parents[1]/'tests/fixtures/legacy_chat.csv').write_text('# '+str(f.relative_to(SRC))+' '+hashlib.sha256(f.read_bytes()).hexdigest()+'\n'+out)
