"""Compile unchanged pinned AddSkillCredits; record nullable counter outcomes."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
REL='Source/ACE.Server/WorldObjects/Player_Skills.cs'
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args()
data=subprocess.check_output(['git','-C',str(a.source),'show',f'{PIN}:{REL}']);source=data.decode('utf-8-sig')
start=source.index('public void AddSkillCredits(');opening=source.index('{',start);depth=0
for end in range(opening,len(source)):
    depth+=(source[end]=='{')-(source[end]=='}')
    if depth==0:
        method=source[start:end+1];break
harness='''using System;
enum PropertyInt { AvailableSkillCredits }
class GameMessagePrivateUpdatePropertyInt { public GameMessagePrivateUpdatePropertyInt(Player p,PropertyInt i,int v) {} }
class Network { public void EnqueueSend(object o) {} }
class Session { public Network Network = new Network(); }
class Player {
 public int? TotalSkillCredits; public int? AvailableSkillCredits; public Session Session = new Session();
 public void SendTransientError(string s) {}
 METHOD
 static string F(int? n)=>n.HasValue?n.Value.ToString():"null";
 static void Main(){
  foreach(var row in new (int?,int?,int)[]{(10,5,3),(10,5,-3),(null,5,2),(null,null,2),(0,0,0),(10,5,-5)}){
   var p=new Player{TotalSkillCredits=row.Item1,AvailableSkillCredits=row.Item2};p.AddSkillCredits(row.Item3);
   Console.WriteLine($"{F(row.Item1)},{F(row.Item2)},{row.Item3},{F(p.TotalSkillCredits)},{F(p.AvailableSkillCredits)}");
  }
 }
}'''.replace('METHOD',method)
with tempfile.TemporaryDirectory(prefix='bace-npc-credits-') as temporary:
    root=Path(temporary);project=root/'Oracle.csproj'
    project.write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><RollForward>Major</RollForward></PropertyGroup></Project>')
    (root/'Program.cs').write_text(harness)
    subprocess.run([a.dotnet,'build','--nologo','-o',str(root/'out'),str(project)],check=True)
    output=subprocess.check_output([a.dotnet,str(root/'out/Oracle.dll')],text=True)
(Path(__file__).parent.parent/'tests/fixtures/training_credits.csv').write_text(f'# official ACE {PIN}; unchanged AddSkillCredits, notification-only stubs\n# sha256 {hashlib.sha256(data).hexdigest()} {REL}\n'+output)
