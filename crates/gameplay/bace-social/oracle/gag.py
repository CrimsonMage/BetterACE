#!/usr/bin/env python3
"""Compile unchanged original ACE GagsTick, controlling only fields and services."""
from pathlib import Path
import hashlib,os,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[4]
source=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Server/WorldObjects/Player_Tick.cs'
text=source.read_text();start=text.index('        public void GagsTick()');end=text.index('{',start)+1;depth=1
while depth:
    if text[end]=='{':depth+=1
    elif text[end]=='}':depth-=1
    end+=1
code='''using System;class Player {public bool IsGagged,gagNoticeSent;public double GagTimestamp=123,GagDuration,CachedHeartbeatInterval;public int Saves,Notice,Restore;void SendGagNotice(){Notice++;}void SendUngagNotice(){Restore++;}void SaveBiotaToDatabase(){Saves++;}
'''+text[start:end]+'''
static void Main(){foreach(bool active in new[]{false,true})foreach(bool noticed in new[]{false,true})foreach(double duration in new[]{0.0,1.0,5.0,299.0,300.0})foreach(double interval in new[]{1.0,5.0,60.0,600.0}){var p=new Player{IsGagged=active,gagNoticeSent=noticed,GagDuration=duration,CachedHeartbeatInterval=interval};p.GagsTick();Console.WriteLine($"{active}|{noticed}|{duration}|{interval}|{p.IsGagged}|{p.gagNoticeSent}|{p.GagDuration}|{p.GagTimestamp}|{p.Notice}|{p.Restore}|{p.Saves}");}}}'''
with tempfile.TemporaryDirectory(prefix='bace-gag-oracle-') as d:
    p=Path(d);(p/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>');(p/'Program.cs').write_text(code)
    out=subprocess.check_output([os.environ.get('DOTNET','/tmp/bace-crafting-dotnet/dotnet'),'run','--project',str(p/'Oracle.csproj'),'-c','Release'],text=True)
    rows=[line for line in out.splitlines() if line.startswith(('True|','False|'))];assert len(rows)==80
    (ROOT/'crates/gameplay/bace-social/tests/fixtures/gag.txt').write_text('# ACE47edade3bd3f6044b676d4eb877c4965c7eda62b Player_Tick.cs sha256 '+hashlib.sha256(text.encode()).hexdigest()+'\n'+'\n'.join(rows)+'\n');print(len(rows),'original gag heartbeat cases')
