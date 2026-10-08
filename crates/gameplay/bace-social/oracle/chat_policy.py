#!/usr/bin/env python3
"""Compile pinned ACE SquelchDB.Contains and TurbineChatHandler adjustment verbatim."""
from pathlib import Path
import hashlib,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[4]; SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
def method(text,sig):
 a=text.index(sig); i=text.index('{',a)+1; d=1
 while d:d+=(text[i]=='{')-(text[i]=='}');i+=1
 return text[a:i]
sq=SRC/'ACE.Server/Network/Structure/SquelchDB.cs';tr=SRC/'ACE.Server/Network/Handlers/TurbineChatHandler.cs';manager=SRC/'ACE.Server/WorldObjects/Managers/SquelchManager.cs';t=tr.read_text();a=t.index('                var adjustedChannelID');b=t.index('                if (channelID != adjustedChannelID)',a)
h='''using System;using System.Collections.Generic;using ACE.Entity.Enum;
class WorldObject{}class Player:WorldObject{public GuidValue Guid=new();public Session Session=new();}class GuidValue{public uint Full=1;}class Session{public string Account="account";}class SquelchInfo{public List<SquelchMask>Filters=new();}
class SquelchManager{LEGAL}
class SquelchDB{public Dictionary<string,uint>Accounts=new();public Dictionary<uint,SquelchInfo>Characters=new();public SquelchInfo Globals=new();CONTAINS}
class Program{static void Main(){uint[]masks={0,4,8,0x40000,0x3ef10cc,0xffffffff};for(int kind=0;kind<3;kind++)foreach(var mask in masks)for(int channel=0;channel<33;channel++){var db=new SquelchDB();if(kind==0)db.Globals.Filters.Add((SquelchMask)mask);if(kind==1)db.Characters.Add(1,new SquelchInfo{Filters=new(){(SquelchMask)mask}});if(kind==2)db.Accounts.Add("account",1);Console.WriteLine($"S,{kind},{mask},{channel},{(db.Contains(new Player(),(ChatMessageType)channel)?1:0)}");}for(uint dispatch=1;dispatch<=2;dispatch++)for(uint channel=0;channel<=12;channel++)for(uint type=0;type<=11;type++){var v=Adjust(dispatch,channel,type);Console.WriteLine($"T,{dispatch},{channel},{type},{v.Item1},{v.Item2}");}}
static (uint,uint)Adjust(uint dispatch,uint channelID,uint type){var chatType=(ChatType)type;var chatBlobDispatchType=(ChatNetworkBlobDispatchType)dispatch;ADJUST return(adjustedChannelID,(uint)adjustedchatType);}}
'''.replace('CONTAINS',method(sq.read_text(),'public bool Contains(')).replace('LEGAL',method(manager.read_text(),'public static bool IsLegalChannel(')).replace('ADJUST',t[a:b])
files=[SRC/('ACE.Entity/Enum/'+n+'.cs')for n in ['ChatMessageType','SquelchMask','ChatType','ChatNetworkBlobDispatchType']]
channel=SRC/'ACE.Server/Entity/TurbineChatChannel.cs'
h='using ACE.Server.Entity;\n'+h
with tempfile.TemporaryDirectory(prefix='chat-policy-') as tmp:
 p=Path(tmp);(p/'Program.cs').write_text(h)
 for i,f in enumerate(files+[channel]):(p/f'source{i}.cs').write_bytes(f.read_bytes())
 (p/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 subprocess.run(['/tmp/bace-crafting-dotnet/dotnet','build','--nologo','-o',str(p/'build')],cwd=p,check=True)
 out=subprocess.check_output(['/tmp/bace-crafting-dotnet/dotnet',str(p/'build/oracle.dll')],text=True)
 Path(__file__).parents[1].joinpath('tests/fixtures/chat_policy.csv').write_text(''.join('# '+str(f.relative_to(SRC))+' '+hashlib.sha256(f.read_bytes()).hexdigest()+'\n'for f in files+[channel,sq,tr,manager])+out)
