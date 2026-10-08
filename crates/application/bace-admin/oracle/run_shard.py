#!/usr/bin/env python3
"""Compile pinned original AdminShardCommands and exact manager methods.
Only transport, scheduling and logging boundaries are stubbed. No Rust is read.
.NET8 required; all oracle dates use explicit UTC, never the current wall clock.
"""
from pathlib import Path
import json, os, subprocess, sys, tempfile, hashlib
ROOT=Path(__file__).resolve().parents[4]
SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
DOTNET=sys.argv[1] if len(sys.argv)>1 else '/tmp/bace-crafting-dotnet/dotnet'
def method(path,signature):
 s=(SRC/path).read_text(encoding='utf-8-sig'); start=s.index(signature); opening=s.index('{',start); depth=1; i=opening+1
 # These selected methods have balanced interpolated braces and no brace comments.
 while depth:
  depth += (s[i]=='{')-(s[i]=='}'); i+=1
 return s[start:i]
rows=[]
def case(command,args=[],interval=60,pending=False,opened=False,name=None):
 rows.append(dict(command=command,args=args,interval=interval,pending=pending,opened=opened,name=name))
for s in ['0','1','60','99999','100000','12345junk','+12345','-00000','-0001',' 12345','123 4','\t42\n','\u00a042','１２３','١٢','💡123','1234💡','','+','4294967295','12\x00','12\x00 ','12 \x00','\x00','12\x00\x00','1_2','1,000']:
 case('set-shutdown-interval',[s],name='Alyssa')
for n in [0,1,59,60,61,90,91,120,121,180,181,3600,7200,86400,90061,99999]:
 for name,message in [(None,[]),(None,['Maintenance','now']),('Alyssa',[]),('Alyssa',['Restart','please'])]:case('shutdown',message,n,name=name)
case('shutdown',['ignored'],pending=True,name='Alyssa')
case('stop-now',['Emergency'],name='Alyssa');case('stop-now',['ignored'],pending=True,name='Alyssa')
case('cancel-shutdown');case('cancel-shutdown',pending=True,name='Alyssa')
for opened in [False,True]:
 for args in [[],['invalid'],['OPEN'],['close'],['close','BOOT'],['close','other'],['open','boot','ignored']]:case('world',args,opened=opened,name='Alyssa')
server='ACE.Server/Managers/ServerManager.cs'
world='ACE.Server/Managers/WorldManager.cs'
players='ACE.Server/Managers/PlayerManager.cs'
with tempfile.TemporaryDirectory(prefix='bace-shard-oracle-') as tmp:
 out=Path(tmp)
 (out/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 copies=['ACE.Server/Command/Handlers/AdminShardCommands.cs','ACE.Server/Command/CommandHandlerAttribute.cs','ACE.Server/Command/CommandHandlerFlag.cs','ACE.Entity/Enum/AccessLevel.cs','ACE.Common/Extensions/DateTimeExtensions.cs']
 for p in copies:(out/Path(p).name).write_bytes((SRC/p).read_bytes())
 stubs=(Path(__file__).parent/'shard_stubs.cs').read_text()
 for key,path,sig in [('SET',server,'public static void SetShutdownInterval('),('CANCEL',server,'public static void CancelShutdown('),('OPEN',world,'internal static void Open('),('CLOSE',world,'internal static void Close('),('BOOT',players,'public static void BootAllPlayers(')]:
  stubs=stubs.replace('/*'+key+'*/',method(path,sig))
 notify=method(server,'private static DateTime NotifyPlayersOfPendingShutdown(')
 (out/'Notify.cs').write_text('using System;using ACE.Server.Managers;using DateTime=OracleDateTime;public static class NotifyOracle { public static long Run(long last,long deadline) => NotifyPlayersOfPendingShutdown(new DateTime(last),new DateTime(deadline)).Millis;\n'+notify+'\n}')
 (out/'Stubs.cs').write_text(stubs)
 (out/'input.json').write_text(json.dumps(rows))
 notices=[]
 for seconds in [7200,3600,2700,1800,900,600,300,120,90,60,30,15,10,5,4,3601,86405]:
  for fraction in [0,1,999]:
   for elapsed in [2000,2001,4000]:notices.append(dict(remaining=seconds*1000+fraction,elapsed=elapsed))
 (out/'notices.json').write_text(json.dumps(notices))
 dates=[-62135596800000,-2208988800000,-1,0,1,951782400000,1709251199000,1728432000000,253402300799999]
 (out/'dates.json').write_text(json.dumps(dates))
 build=subprocess.run([DOTNET,'build','--nologo','-o',str(out/'build')],cwd=out,capture_output=True,text=True)
 if build.returncode: print(build.stdout,build.stderr);build.check_returncode()
 run=subprocess.run([DOTNET,str(out/'build/oracle.dll')],cwd=out,capture_output=True,text=True,check=True,env={**os.environ,'TZ':'UTC'})
 fixture=ROOT/'crates/application/bace-runtime/tests/fixtures/shard_commands.json'
 fixture.parent.mkdir(exist_ok=True)
 data=json.loads(run.stdout);data['provenance']={p:hashlib.sha256((SRC/p).read_bytes()).hexdigest() for p in copies+[server,world,players]}
 fixture.write_text(json.dumps(data,indent=2,ensure_ascii=True)+'\n');print(fixture)
