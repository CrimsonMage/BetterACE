#!/usr/bin/env python3
"""Compile the verbatim pinned ACE school-recovery branch with explicit clock/RNG adapters."""
import argparse, subprocess, tempfile, hashlib, urllib.request
from pathlib import Path
p=argparse.ArgumentParser(); p.add_argument('--source',type=Path,required=True); p.add_argument('--dotnet',required=True); a=p.parse_args()
pin='47edade3bd3f6044b676d4eb877c4965c7eda62b'
rel='Source/ACE.Server/WorldObjects/Player_Magic.cs'
data=(a.source/rel).read_bytes()
assert data==urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{pin}/{rel}',timeout=30).read()
s=data.decode('utf-8-sig'); start=s.index('            if (spell.School == MagicSchool.VoidMagic && LastSuccessCast_School')
opening=s.index('{',start); depth=0
for end in range(opening,len(s)):
 depth+=(s[end]=='{')-(s[end]=='}')
 if depth==0: break
branch=s[start:end+1]
program='''using System;
enum MagicSchool { WarMagic=1,LifeMagic=2,CreatureEnchantment=3,ItemEnchantment=4,VoidMagic=5 }
enum CastingPreCheckStatus { Success,CastFailed }
enum ChatMessageType { Magic }
record Spell(MagicSchool School);
record GameMessageSystemChat(string Message,ChatMessageType Type);
static class ThreadSafeRandom { public static float Draw; public static float Next(float a,float b)=>a+Draw*(b-a); }
static class Time { public static double Now; public static double GetUnixTime()=>Now; }
class Network { public void EnqueueSend(GameMessageSystemChat message) {} }
class SessionType { public Network Network=new(); }
class Program {
 static SessionType Session=new();
 static MagicSchool LastSuccessCast_School; static double LastSuccessCast_Time;
 static bool Check(Spell spell) { var castingPreCheckStatus=CastingPreCheckStatus.Success;
 // BRANCH
 return castingPreCheckStatus==CastingPreCheckStatus.CastFailed;
 }
 static void Main() { foreach(var previous in new[]{MagicSchool.WarMagic,MagicSchool.VoidMagic,MagicSchool.LifeMagic})
 foreach(var next in new[]{MagicSchool.WarMagic,MagicSchool.VoidMagic,MagicSchool.LifeMagic})
 foreach(var age in new[]{0.0,2.999,3.0,4.0,4.999,5.0}) foreach(var draw in new[]{0f,0.5f,1f}) {
 LastSuccessCast_School=previous; LastSuccessCast_Time=10; Time.Now=10+age; ThreadSafeRandom.Draw=draw;
 Console.WriteLine($"{(int)previous},{(int)next},{age:R},{draw:R},{(Check(new Spell(next))?1:0)}");
 } }
}
'''.replace('// BRANCH',branch)
with tempfile.TemporaryDirectory(prefix='bace-recovery-oracle-') as td:
 root=Path(td); (root/'Program.cs').write_text(program); (root/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 subprocess.run([a.dotnet,'build',str(root/'oracle.csproj'),'-o',str(root/'out'),'--nologo','-v:q'],check=True)
 output=subprocess.check_output([a.dotnet,str(root/'out/oracle.dll')],text=True)
path=Path(__file__).resolve().parent.parent/'tests/fixtures/recovery.csv'
path.write_text(f'# ACE {pin}; AGPL-3.0-only ACE contributors\n# sha256 {hashlib.sha256(data).hexdigest()} {rel}\n# Verbatim school recovery branch; explicit clock and RNG, messages discarded.\n'+output)
