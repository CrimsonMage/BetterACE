#!/usr/bin/env python3
"""Independent verbatim pinned ACE skill transition/bonus oracle; synthetic data."""
import argparse, hashlib, subprocess, tempfile, urllib.request
from pathlib import Path
PIN = '47edade3bd3f6044b676d4eb877c4965c7eda62b'
p = argparse.ArgumentParser(); p.add_argument('--source', type=Path, required=True); p.add_argument('--dotnet', required=True); a = p.parse_args()
def method(s, signature):
    start=s.index(signature); opening=s.index('{',start); depth=0
    for i in range(opening,len(s)):
        depth+=(s[i]=='{')-(s[i]=='}')
        if depth==0:return s[start:i+1]
    raise ValueError(signature)
files={}; headers=[f'# official ACE {PIN}; verbatim methods; synthetic adapters; AGPL-3.0-only']
for path in ['Source/ACE.Server/WorldObjects/Player_Skills.cs','Source/ACE.Server/WorldObjects/Entity/CreatureSkill.cs','Source/ACE.Entity/Enum/Skill.cs','Source/ACE.Server/WorldObjects/Player_Xp.cs']:
    data=(a.source/path).read_bytes()
    if data != urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{path}',timeout=30).read(): raise ValueError(path)
    files[Path(path).name]=data.decode('utf-8-sig'); headers.append(f'# sha256 {hashlib.sha256(data).hexdigest()} {path}')
s=files['Player_Skills.cs']
methods='\n'.join(method(s,x) for x in ['public bool TrainSkill(Skill skill, int creditsSpent,','public bool SpecializeSkill(Skill skill, int creditsSpent,','public bool UntrainSkill(','public bool UnspecializeSkill(','public bool ResetSkill(','public static int CalcSkillRank(','public static List<uint> GetSkillXPTable(','public static bool IsSkillUntrainable(','public bool IsSkillSpecializedViaAugmentation('])
methods+='\n'+method(files['Player_Xp.cs'],'public void RefundXP(')
# Lists are source bodies too, preserving skill membership independently.
lists='\n'.join(s[s.index('public static '+kind+'<Skill> '+name):s.index(';',s.index('public static '+kind+'<Skill> '+name))+1] for name,kind in [('AlwaysTrained','List'),('AugSpecSkills','List'),('MeleeSkills','HashSet'),('MissileSkills','HashSet'),('MagicSkills','HashSet')])
source=(Path(__file__).parent/'skill_harness.cs').read_text().replace('// ENUM',method(files['Skill.cs'],'public enum Skill')).replace('// METHODS',methods).replace('// LISTS',lists)
source=source.replace('// BONUSES','\n'.join(method(files['CreatureSkill.cs'],x) for x in ['public uint GetAugBonus_Base(','public uint GetAugBonus_Current(']))
with tempfile.TemporaryDirectory(prefix='bace-skill-oracle-') as td:
    b=Path(td); (b/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'); (b/'Program.cs').write_text(source)
    subprocess.run([a.dotnet,'build','--nologo','-o',str(b/'out'),str(b/'Oracle.csproj')],check=True)
    output=subprocess.check_output([a.dotnet,str(b/'out/Oracle.dll')],text=True)
(Path(__file__).parent.parent/'tests/fixtures/skills.csv').write_text('\n'.join(headers)+'\n'+output)
