#!/usr/bin/env python3
"""Original C# NameWithMaterial/BroadcastTinkering and source Round extension."""
from pathlib import Path
import re,tempfile,subprocess,hashlib
ROOT=Path(__file__).resolve().parents[5];SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
def method(s,sig):
 a=s.index(sig);i=s.index('{',a)+1;n=1
 while n:n+=(s[i]=='{')-(s[i]=='}');i+=1
 return s[a:i]
p=SRC/'ACE.Server/Managers/RecipeManager.cs';q=SRC/'ACE.Server/WorldObjects/WorldObject_Properties.cs';roundp=SRC/'ACE.Common/Extensions/FloatExtensions.cs'
broadcast=method(p.read_text(),'public static void BroadcastTinkering(')
name=method(q.read_text(),'public string GetNameWithMaterial(')
rounder=method(roundp.read_text(),'public static int Round(this double')
code='''using System.Text.RegularExpressions;
class GameMessageSystemChat {public string Text; public GameMessageSystemChat(string t,ChatMessageType c){Text=t;}}
enum ChatMessageType{Craft}
class Logger{public void Info(string s){}}
class WorldObject{public const float LocalBroadcastRange=30;public string Name,Inscription,ScribeName;public float? Workmanship;public uint? MaterialType;public string GetPluralName()=>Name;public string NameWithMaterial=>GetNameWithMaterial();'''+name+'''}
class Player:WorldObject{public void EnqueueBroadcast(GameMessageSystemChat m,float r,ChatMessageType c){Console.WriteLine(m.Text);}}
class RecipeManager {static Logger log=new();public static string GetMaterialName(uint id)=>id==64?"Steel":"Gold";'''+broadcast+'''}
static class Extensions{'''+rounder+'''}
class Program{static void Main(){foreach(var chance in new double[]{0,0.005,0.325,0.33,0.375,0.38,0.995,1})Console.WriteLine($"C\\t{chance:R}\\t{(chance*100).Round()}");
for(int n=0;n<8;n++){var p=new Player{Name="Alice"};var tool=new WorldObject{Name=n%2==0?"Salvage (100)":"Steel Salvaged",MaterialType=64,Workmanship=new float[]{1,2.675f,5,9.99f}[n%4]};var target=new WorldObject{Name=n%2==0?"Gold Sword":"Sword",MaterialType=n%2==0?2u:null,Inscription=n%3==0?"inscribed":null,ScribeName="Bob"};Console.Write($"T\\t{n}\\t");RecipeManager.BroadcastTinkering(p,tool,target,1,n<4);}}}
'''
with tempfile.TemporaryDirectory(prefix='bace-tinker-messages-') as tmp:
 d=Path(tmp);(d/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>');(d/'Program.cs').write_text(code);dotnet='/tmp/bace-crafting-dotnet/dotnet';r=subprocess.run([dotnet,'build','--nologo','-o',str(d/'build')],cwd=d,capture_output=True,text=True)
 if r.returncode:print(r.stdout);r.check_returncode()
 r=subprocess.run([dotnet,str(d/'build/oracle.dll')],cwd=d,capture_output=True,text=True,check=True)
 dest=ROOT/'crates/gameplay/bace-crafting/tests/fixtures/tinker_messages.tsv';dest.write_text('# Original ACE 47edade3 BroadcastTinkering/NameWithMaterial/FloatExtensions.Round\n'+''.join('# sha256 '+hashlib.sha256(f.read_bytes()).hexdigest()+' '+str(f.relative_to(SRC))+'\n' for f in [p,q,roundp])+r.stdout);print(dest)
