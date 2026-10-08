#!/usr/bin/env python3
"""Execute four original pinned ACE fellowship methods in a minimal C# host."""
import argparse,csv,hashlib,pathlib,subprocess,tempfile
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
p=argparse.ArgumentParser();p.add_argument('checkout',type=pathlib.Path);p.add_argument('--dotnet',default='dotnet');a=p.parse_args()
s=(a.checkout/'Source/ACE.Server/Entity/Fellowship.cs').read_text()
def method(signature):
    start=s.index(signature);brace=s.index('{',start);depth=1;i=brace+1
    while depth:
        depth+=(s[i]=='{')-(s[i]=='}');i+=1
    return s[start:i]
methods='\n'.join(method(x) for x in ('private void CalculateXPSharing()', 'public void SplitXp(', 'internal double GetMemberSharePercent()', 'public double GetDistanceScalar('))
harness='''using System;using System.Linq;using System.Collections.Generic;using System.Globalization;
[Flags]enum ShareType{None=0,Fellowship=1,Allegiance=2,All=3}enum XpType{Kill,Quest,Fellowship}
record Setting<T>(T Item);static class PropertyManager {public static Setting<long> GetLong(string k)=>new(50);public static Setting<bool> GetBool(string k)=>new(false);}
class Position{public bool Indoors;public uint Landblock;public float X;public float Distance2D(Position p)=>Math.Abs(X-p.X);}
class Player{public int? Level;public Position Location=new();public long NextXp,Granted;public long GetXPToNextLevel(int level)=>NextXp;public void GrantXP(long amount,XpType type,ShareType share){Granted+=amount;}}
static class PlayerManager{public static Dictionary<uint,Player> Players=new();public static Player? GetOnlinePlayer(uint id)=>Players.GetValueOrDefault(id);}
class Fellowship{public bool ShareXP,EvenShare,DesiredShareXP=true;public uint FellowshipLeaderGuid=1;public const int MaxDistance=600;Dictionary<uint,Player> GetFellowshipMembers()=>PlayerManager.Players;
'''+methods+'''
public void Run(ulong amount,bool quest){CalculateXPSharing();if(ShareXP)SplitXp(amount,quest?XpType.Quest:XpType.Kill,ShareType.All,PlayerManager.Players[1]);else PlayerManager.Players[1].Granted=(long)amount;}
}
class Program{static void Main(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;string? line;while((line=Console.ReadLine())!=null){var f=line.Split(',');var amount=ulong.Parse(f[0]);var quest=f[1]=="1";int count=int.Parse(f[2]);PlayerManager.Players.Clear();for(int i=0;i<count;i++){int j=3+5*i;PlayerManager.Players[(uint)i+1]=new Player{Level=int.Parse(f[j]),NextXp=long.Parse(f[j+1]),Location=new Position{X=float.Parse(f[j+2]),Indoors=f[j+3]=="1",Landblock=uint.Parse(f[j+4])}};}var fellow=new Fellowship();fellow.Run(amount,quest);Console.WriteLine((fellow.ShareXP?1:0)+","+(fellow.EvenShare?1:0)+","+string.Join(",",PlayerManager.Players.Values.Select(p=>p.Granted)));}}}
'''
rows=[]
for count in range(1,10):
 for quest in (0,1):
  for mode in range(5):
   members=[]
   for i in range(count):
    level=100+i*10 if mode==0 else 20+(0 if i==0 else [0,5,6,10,11][mode])
    distance=0 if i==0 else [0,600,750,1199.9,1200][mode]
    members.extend((level,1000*(i+1),distance,0,1))
   rows.append((1000003,quest,count,*members))
for indoors,block in ((1,1),(1,2)):
 rows.append((101,0,2,20,1000,0,1,1,20,1000,0,indoors,block))
with tempfile.TemporaryDirectory(prefix='bace-ace-fellow-') as tmp:
 tmp=pathlib.Path(tmp);(tmp/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><Nullable>enable</Nullable></PropertyGroup></Project>');(tmp/'Program.cs').write_text(harness)
 subprocess.run([a.dotnet,'build',str(tmp/'oracle.csproj'),'-o',str(tmp/'out'),'--nologo','-v:q'],check=True,stdout=subprocess.DEVNULL)
 answers=subprocess.check_output([a.dotnet,str(tmp/'out/oracle.dll')],input=''.join(','.join(map(str,row))+'\n' for row in rows),text=True).splitlines()
assert len(answers)==len(rows)
out=pathlib.Path(__file__).parents[1]/'tests/fixtures/ace_rewards.csv'
with out.open('w')as f:
 f.write(f'# ACE {PIN} Entity/Fellowship.cs extracted-sha256={hashlib.sha256(methods.encode()).hexdigest()}\n')
 for row,answer in zip(rows,answers):f.write(','.join(map(str,row))+';'+answer+'\n')
print(f'Wrote {len(rows)} original C# reward vectors')
