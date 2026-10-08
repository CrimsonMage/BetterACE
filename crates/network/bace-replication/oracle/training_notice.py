#!/usr/bin/env python3
"""Execute pinned ACE HandleActionTrainSkill and Skill.ToSentence unchanged.
AGPL-3.0-only; source attribution remains ACEmulator/ACE at the pinned revision.
Only surrounding DAT/log/transport/skill mutation adapters are test doubles.
"""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile

PIN = "47edade3bd3f6044b676d4eb877c4965c7eda62b"
p = argparse.ArgumentParser()
p.add_argument("--repo", type=Path, required=True)
p.add_argument("--dotnet", required=True)
a = p.parse_args()
root = Path(__file__).resolve().parents[1]
paths = ["Source/ACE.Entity/Enum/Skill.cs", "Source/ACE.Server/WorldObjects/Player_Skills.cs"]
sources = {path: subprocess.check_output(["git", "-C", str(a.repo), "show", f"{PIN}:{path}"]) for path in paths}
s = sources[paths[1]].decode()
start = s.index("public bool HandleActionTrainSkill(")
opening = s.index("{", start)
depth = 0
for end in range(opening, len(s)):
    depth += (s[end] == "{") - (s[end] == "}")
    if depth == 0:
        method = s[start:end + 1]
        break
harness = '''using System; using System.Collections.Generic; using ACE.Entity.Enum;
enum PropertyInt { AvailableSkillCredits } enum ChatMessageType { Advancement = 13 }
class Log { public void Warn(string text) {} }
class SkillBase { public int TrainedCost = 0; }
class SkillTable { public Dictionary<uint,SkillBase> SkillBaseHash=new(); }
class Portal { public SkillTable SkillTable=new(); }
static class DatManager { public static Portal PortalDat=new(); }
class GameMessagePrivateUpdateSkill { public GameMessagePrivateUpdateSkill(Player p, object s) {} }
class GameMessagePrivateUpdatePropertyInt { public GameMessagePrivateUpdatePropertyInt(Player p, PropertyInt k, int v) {} }
class GameMessageSystemChat { public string Text; public GameMessageSystemChat(string text, ChatMessageType type) {Text=text;} }
class Network { public string Text=""; public void EnqueueSend(params object[] args) {foreach(var o in args) if(o is GameMessageSystemChat c) Text=c.Text;} }
class Session { public Network Network=new(); }
class Player {
 public string Name="test"; public Log log=new(); public int? AvailableSkillCredits; public bool Success;
 public Session Session=new(); public object GetCreatureSkill(Skill s)=>new(); public bool TrainSkill(Skill s,int c)=>Success;
 // METHOD
}
class Program {static void Main(){ foreach(uint skill in System.Linq.Enumerable.Range(0,55)) foreach(int credits in new[]{0,1,20,int.MaxValue}) foreach(bool success in new[]{false,true}) {
 DatManager.PortalDat.SkillTable.SkillBaseHash[skill]=new();
 var p=new Player{AvailableSkillCredits=credits,Success=success};p.HandleActionTrainSkill((Skill)skill,0);
 Console.WriteLine($"{skill}|{credits}|{(success?1:0)}|{p.Session.Network.Text}");
}}}
'''.replace("// METHOD", method)
with tempfile.TemporaryDirectory(prefix="bace-training-notice-") as work:
    work = Path(work)
    (work / "Oracle.csproj").write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><EnableDefaultCompileItems>true</EnableDefaultCompileItems></PropertyGroup></Project>')
    (work / "Skill.cs").write_bytes(sources[paths[0]])
    (work / "Program.cs").write_text(harness)
    subprocess.run([a.dotnet, "build", str(work / "Oracle.csproj"), "--nologo", "-v:q"], check=True)
    output = subprocess.check_output([a.dotnet, str(work / "bin/Debug/net8.0/Oracle.dll")])
    assert len(output.splitlines()) == 440
    fixtures = root / "tests/fixtures"
    fixtures.mkdir(exist_ok=True)
    (fixtures / "training_notice.txt").write_bytes(output)
    provenance = f'ACE {PIN}\nUnchanged HandleActionTrainSkill plus complete Skill.cs. Surrounding state/transport stubs only.\n'
    provenance += '\n'.join(f'{hashlib.sha256(data).hexdigest()}  {path}' for path, data in sources.items()) + '\n'
    (fixtures / "training_notice.provenance").write_text(provenance)
