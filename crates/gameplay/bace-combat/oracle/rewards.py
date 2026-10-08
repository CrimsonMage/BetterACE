#!/usr/bin/env python3
"""Run verbatim pinned OnDeath_GrantXP against synthetic damage-history adapters."""
import argparse,hashlib,subprocess,tempfile,urllib.request
from pathlib import Path
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',default='dotnet');a=p.parse_args()
rel='Source/ACE.Server/WorldObjects/Creature_Death.cs'
data=(a.source/rel).read_bytes();official=urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{rel}',timeout=30).read()
if data!=official:raise ValueError('source mismatch')
s=data.decode('utf-8-sig');start=s.index('public void OnDeath_GrantXP()');opening=s.index('{',start);depth=0
for i in range(opening,len(s)):
 depth+=(s[i]=='{')-(s[i]=='}')
 if depth==0:method=s[start:i+1];break
harness=r'''
using System;using System.Linq;using System.Collections.Generic;using System.Globalization;
enum PlayerKillerStatus {NPK,PKLite} enum XpType {Kill}
class Info {public float TotalDamage;public object PetOwner=null;public Creature Attacker;public Creature TryGetAttacker()=>Attacker;public Player TryGetPetOwner()=>null;}
class History {public Dictionary<int,Info> TotalDamage=new();public float TotalHealth=>TotalDamage.Values.Sum(i=>i.TotalDamage);}
class Creature {public History DamageHistory=new();public int? XpOverride;public long? LuminanceAward=null;public PlayerKillerStatus PlayerKillerStatus=PlayerKillerStatus.NPK;
// METHOD
}
class Player:Creature {public long? Earned;public void EarnXP(long amount,XpType type){Earned=amount;}public void EarnLuminance(long amount,XpType type){}}
class Program {static void Main(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
 float[][] cases={new[]{1f,3f},new[]{0f,0f},new[]{100000000f,1f,1f,1f,1f,1f,1f,1f,1f},new[]{1f,2f,3f,4f},new[]{3f,7f,11f},new[]{16777216f,1f,1f,1f}};
 foreach(var amounts in cases)foreach(int xp in new[]{0,1,10,100,1000000000,int.MaxValue})foreach(bool ineligible in new[]{false,true}){
  var victim=new Creature{XpOverride=xp};var players=new List<Player>();var mask="";
  for(int i=0;i<amounts.Length;i++){var player=new Player();players.Add(player);bool eligible=!ineligible||i!=amounts.Length-1;mask+=eligible?"1":"0";victim.DamageHistory.TotalDamage[i]=new Info{TotalDamage=amounts[i],Attacker=eligible?player:new Creature()};}
  victim.OnDeath_GrantXP();
  Console.WriteLine($"{xp},{string.Join(';',amounts.Select(x=>x.ToString("R")))},{mask},{string.Join(';',players.Select(p=>p.Earned.HasValue?p.Earned.Value.ToString():"-"))}");
 }
}}
'''.replace('// METHOD',method)
with tempfile.TemporaryDirectory(prefix='bace-reward-oracle-') as td:
 b=Path(td);(b/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>');(b/'Program.cs').write_text(harness)
 subprocess.run([a.dotnet,'build','--nologo','-o',str(b/'out'),str(b/'Oracle.csproj')],check=True)
 result=subprocess.check_output([a.dotnet,str(b/'out/Oracle.dll')],text=True)
 out=Path(__file__).resolve().parents[1]/'tests/fixtures/rewards.csv';out.write_text(f'# Official ACE {PIN}; AGPL-3.0-only\n# sha256 {hashlib.sha256(data).hexdigest()} {rel}\n'+result)
