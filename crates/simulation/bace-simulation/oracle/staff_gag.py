#!/usr/bin/env python3
"""Original ACE gag commands and PlayerManager methods, with service/property stubs."""
from pathlib import Path
import hashlib,os,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[4]
base=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Server'
files=[base/'Command/Handlers/AdminCommands.cs',base/'Managers/PlayerManager.cs']
texts=[p.read_text() for p in files]
def method(text,marker):
 s=text.index(marker);e=text.index('{',s)+1;depth=1
 while depth:
  if text[e]=='{':depth+=1
  elif text[e]=='}':depth-=1
  e+=1
 return text[s:e]
code='''using System;
namespace ACE.Entity.Enum.Properties {enum PropertyBool{IsGagged}enum PropertyFloat{GagTimestamp,GagDuration}}
namespace Common {static class Time{public static double GetUnixTime()=>123;}}
enum ChatMessageType{WorldBroadcast=20}
class Player{public string Name="Staff";public bool Active;public double Stamp,Duration;public int Saves;public void SetProperty(ACE.Entity.Enum.Properties.PropertyBool p,bool v){Active=v;}public void SetProperty(ACE.Entity.Enum.Properties.PropertyFloat p,double v){if(p==ACE.Entity.Enum.Properties.PropertyFloat.GagTimestamp)Stamp=v;else Duration=v;}public void RemoveProperty(ACE.Entity.Enum.Properties.PropertyBool p){Active=false;}public void RemoveProperty(ACE.Entity.Enum.Properties.PropertyFloat p){SetProperty(p,0);}public void SaveBiotaToDatabase(){Saves++;}}
class Session{public Player Player=new();}static class CommandHandlerHelper{public static string Text;public static int Type;public static void WriteOutputInfo(Session s,string text,ChatMessageType type){Text=text;Type=(int)type;}}
static class PlayerManager{public static bool Found;public static Player Target;public static string Audit="";static Player FindByName(string name)=>Found?Target:null;static void BroadcastToAuditChannel(Player p,string text){Audit=text;}
'''+method(texts[1],'        public static bool GagPlayer(')+method(texts[1],'        public static bool UnGagPlayer(')+'''}class Program{
'''+method(texts[0],'        public static void HandleGag(')+method(texts[0],'        public static void HandleUnGag(')+'''
static string Hex(string s)=>Convert.ToHexString(System.Text.Encoding.UTF8.GetBytes(s));
static void Main(){foreach(bool enable in new[]{false,true})foreach(bool found in new[]{false,true}){PlayerManager.Found=found;PlayerManager.Target=new Player{Active=!enable,Stamp=99,Duration=23};PlayerManager.Audit="";if(enable)HandleGag(new Session(),"Staff");else HandleUnGag(new Session(),"Staff");var p=PlayerManager.Target;Console.WriteLine($"{enable}|{found}|{p.Active}|{p.Stamp}|{p.Duration}|{p.Saves}|{Hex(PlayerManager.Audit)}|{Hex(CommandHandlerHelper.Text)}|{CommandHandlerHelper.Type}");}}}'''
with tempfile.TemporaryDirectory(prefix='bace-staff-gag-') as d:
 p=Path(d);(p/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>');(p/'Program.cs').write_text(code)
 out=subprocess.check_output([os.environ.get('DOTNET','/tmp/bace-crafting-dotnet/dotnet'),'run','--project',str(p/'Oracle.csproj'),'-c','Release'],text=True)
 rows=[l for l in out.splitlines() if l.startswith(('True|','False|'))];assert len(rows)==4
 (ROOT/'crates/simulation/bace-simulation/tests/fixtures/staff_gag.txt').write_text('# ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b '+','.join(p.name+':'+hashlib.sha256(t.encode()).hexdigest() for p,t in zip(files,texts))+'\n'+'\n'.join(rows)+'\n');print(len(rows),'original command/manager vectors')
