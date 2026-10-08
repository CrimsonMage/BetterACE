#!/usr/bin/env python3
"""Pinned unchanged Player portal physics methods; synthetic flag/time adapters.
AGPL-3.0-only; original method copyright ACEmulator contributors.
"""
from pathlib import Path
import os
import hashlib, subprocess, tempfile
ROOT=Path(__file__).resolve().parents[4]
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
source=ROOT/'.reference'/('ACE-'+PIN)/'Source/ACE.Server/WorldObjects/Player_Location.cs'
text=source.read_text()
def method(signature):
    a=text.index(signature);i=text.index('{',a)+1;depth=1
    while depth:depth+=(text[i]=='{')-(text[i]=='}');i+=1
    return text[a:i]
methods='\n'.join(method(s) for s in ['public void DoTeleportPhysicsStateChanges()','public void OnTeleportComplete()'])
program=r'''using System;
enum CloakStatus { Undef,Off,On,Player,Creature }
class Landblock{public bool CreateWorldObjectsCompleted=true;}
class ActionChain{public void AddDelaySeconds(double n){}public void AddAction(object p,Action a){}public void EnqueueChain(){}}
class Time{public static double GetUnixTime()=>1234;}
class Player{public uint State;public int Broadcasts;public bool Teleporting=true;public CloakStatus CloakStatus;
public Landblock CurrentLandblock=new();public double LastTeleportStartTimestamp=1,LastPortalTeleportTimestamp=1;
public bool? Hidden {get=>(State&0x10)!=0;set{if(value==true)State|=0x10;else State&=~0x10u;}}
public bool? IgnoreCollisions{get=>(State&0x4000)!=0;set{if(value==true)State|=0x4000;else State&=~0x4000u;}}
public bool? ReportCollisions{get=>(State&8)!=0;set{if(value==true)State|=8;else State&=~8u;}}
void EnqueueBroadcastPhysicsState(){Broadcasts++;}void CheckMonsters(){}void CheckHouse(){}
METHODS
}
class Program{static void Main(){foreach(uint flags in new uint[]{0,8,0x10,0x4000,0x4010,0x4018,0x404418,0xffffffff})for(int cloak=0;cloak<5;cloak++)for(int phase=0;phase<2;phase++){
var p=new Player{State=flags,CloakStatus=(CloakStatus)cloak};if(phase==0)p.DoTeleportPhysicsStateChanges();else p.OnTeleportComplete();Console.WriteLine($"{flags},{cloak},{phase},{p.State},{p.Broadcasts}");
}}}
'''.replace('METHODS',methods)
with tempfile.TemporaryDirectory(prefix='bace-portal-state-') as tmp:
    d=Path(tmp);(d/'Program.cs').write_text(program);(d/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><WarningLevel>0</WarningLevel></PropertyGroup></Project>')
    dotnet = os.environ.get("BACE_DOTNET", "dotnet")
    subprocess.run([dotnet,'build','--nologo','-o',str(d/'build')],cwd=d,check=True)
    output=subprocess.check_output([dotnet,str(d/'build/oracle.dll')],text=True)
    (Path(__file__).resolve().parents[1]/'tests/fixtures/portal_state.csv').write_text('# official ACE '+PIN+'\n# Player_Location.cs sha256='+hashlib.sha256(source.read_bytes()).hexdigest()+'\n# before,cloak,phase,after,broadcast_count\n'+output)
    print('original portal state vectors:',len(output.splitlines()))
