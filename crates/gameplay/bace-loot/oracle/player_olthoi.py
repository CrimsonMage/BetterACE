#!/usr/bin/env python3
"""Execute original ACE player slag/gland methods with explicit draw tape."""
from pathlib import Path
import hashlib,subprocess,tempfile
root=Path(__file__).resolve().parents[4];pin='47edade3bd3f6044b676d4eb877c4965c7eda62b'
path=root/'.reference'/('ACE-'+pin)/'Source/ACE.Server/Factories/LootGenerationFactory_OlthoiPlay.cs'
s=path.read_text()
def method(marker):
 a=s.index(marker);i=s.index('{',a);depth=1;j=i+1
 while depth:
  depth+=(s[j]=='{')-(s[j]=='}');j+=1
 return s[a:j]
roundpath=root/'.reference'/('ACE-'+pin)/'Source/ACE.Common/Extensions/FloatExtensions.cs'
roundsource=roundpath.read_text();a=roundsource.index('public static int Round(this float');b=roundsource.index('\n        }',a)+10
program='''using System;using System.Collections.Generic;
static class Extensions{ROUND}
class Player{public int? Level,OlthoiLootTimestamp;}
class WorldObject{public uint Id;public int Stack=1;public void SetStackSize(int v){Stack=v;}}
static class WorldObjectFactory{public static WorldObject CreateNewWorldObject(uint id)=>new(){Id=id};}
static class Time{public static int Now;public static double GetUnixTime()=>Now;}
static class ThreadSafeRandom{public static int Case,Calls,Units;public static int Next(int a,int b){Calls++;return Case%2==0?a:b;}public static double Next(float a,float b){Calls++;return Units++<(Case%3)?0.01:0.5;}}
static class Factory{static uint slagWcid=43491,glandWcid=43747;static TimeSpan pvpSlagTimer=TimeSpan.FromHours(1);SLAG TIER GLAND}
class Program{static void Main(){int c=0;foreach(int level in new[]{99,100,134,135,184,185,274,275})foreach(int elapsed in new[]{0,120,1800,3599,3600,4000})foreach(bool vitae in new[]{false,true}){ThreadSafeRandom.Case=c;ThreadSafeRandom.Calls=0;ThreadSafeRandom.Units=0;Time.Now=10000;var p=new Player{Level=level,OlthoiLootTimestamp=10000-elapsed};var result=Factory.RollSlag(p,vitae);Console.WriteLine($"S|{c}|{level}|{elapsed}|{vitae}|{result?.Stack??0}|{p.OlthoiLootTimestamp}|{ThreadSafeRandom.Calls}");c++;}foreach(bool vitae in new[]{false,true})for(c=0;c<2;c++){ThreadSafeRandom.Case=c;ThreadSafeRandom.Calls=0;var result=Factory.RollGland(new Player(),vitae);Console.WriteLine($"G|{c}|{vitae}|{result?.Id??0}|{ThreadSafeRandom.Calls}");}}}
'''.replace('ROUND',roundsource[a:b]).replace('SLAG',method('public static WorldObject RollSlag(Player')).replace('TIER',method('private static int GetTierHeuristic')).replace('GLAND',method('public static WorldObject RollGland'))
with tempfile.TemporaryDirectory(prefix='bace-player-olthoi-') as tmp:
 d=Path(tmp);(d/'Program.cs').write_text(program);(d/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><WarningLevel>0</WarningLevel></PropertyGroup></Project>')
 dotnet='/tmp/bace-crafting-dotnet/dotnet';subprocess.run([dotnet,'build','--nologo','-o',str(d/'build')],cwd=d,check=True)
 out=subprocess.check_output([dotnet,str(d/'build/oracle.dll')],text=True)
 (Path(__file__).resolve().parents[1]/'tests/fixtures/player_olthoi.trace').write_text('# ACE '+pin+'\n# '+path.name+' sha256='+hashlib.sha256(path.read_bytes()).hexdigest()+'\n# FloatExtensions.cs sha256='+hashlib.sha256(roundpath.read_bytes()).hexdigest()+'\n'+out)
 print(len(out.splitlines()),'original source cases')
