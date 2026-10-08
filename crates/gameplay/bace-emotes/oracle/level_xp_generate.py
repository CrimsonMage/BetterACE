"""Compile the pinned original level-proportional XP method, without rewriting it."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile

PIN = '47edade3bd3f6044b676d4eb877c4965c7eda62b'
REL = 'Source/ACE.Server/WorldObjects/Player_Xp.cs'
p = argparse.ArgumentParser()
p.add_argument('--source', type=Path, required=True)
p.add_argument('--dotnet', required=True)
a = p.parse_args()
data = subprocess.check_output(['git', '-C', str(a.source), 'show', f'{PIN}:{REL}'])
source = data.decode('utf-8-sig')
start = source.index('public void GrantLevelProportionalXp(')
opening = source.index('{', start)
depth = 0
for end in range(opening, len(source)):
    depth += (source[end] == '{') - (source[end] == '}')
    if depth == 0:
        method = source[start:end + 1]
        break
harness = '''using System;
enum XpType { Quest }
enum ShareType { None, Allegiance, All }
class Player {
 public int? Level = 1; public long Next; public long Result; public ShareType Sharing;
 public long GetXPBetweenLevels(int a,int b) => Next;
 public void EarnXP(long amount,XpType kind,ShareType sharing) { Result=amount; Sharing=sharing; }
 METHOD
 static void Main() {
  foreach(var row in new (long,double,long,long)[]{(100,0.25,0,0),(101,0.5,0,0),(103,0.5,0,0),(100,0.8,0,60),(100,0.2,30,0),(100,0.2,30,10),(100,0,0,0)}) {
   var p=new Player{Next=row.Item1};p.GrantLevelProportionalXp(row.Item2,row.Item3,row.Item4);
   Console.WriteLine($"{row.Item1},{row.Item2.ToString(System.Globalization.CultureInfo.InvariantCulture)},{row.Item3},{row.Item4},{p.Result},{p.Sharing}");
  }
 }
}'''.replace('METHOD', method)
with tempfile.TemporaryDirectory(prefix='bace-npc-level-xp-') as temporary:
    root = Path(temporary)
    project = root / 'Oracle.csproj'
    project.write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><RollForward>Major</RollForward></PropertyGroup></Project>')
    (root / 'Program.cs').write_text(harness)
    subprocess.run([a.dotnet, 'build', '--nologo', '-o', str(root / 'out'), str(project)], check=True)
    output = subprocess.check_output([a.dotnet, str(root / 'out/Oracle.dll')], text=True)
destination = Path(__file__).parent.parent / 'tests/fixtures/level_xp.csv'
destination.write_text(f'# official ACE {PIN}; unchanged GrantLevelProportionalXp; XP table and EarnXP recorder are explicit stubs\n# sha256 {hashlib.sha256(data).hexdigest()} {REL}\n' + output)
