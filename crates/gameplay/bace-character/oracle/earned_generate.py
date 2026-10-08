"""Pinned unchanged UpdateXpAndLevel+CheckForLevelup, synthetic DAT/network adapters."""
import argparse,hashlib,subprocess,tempfile,urllib.request
from pathlib import Path
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args();rel='Source/ACE.Server/WorldObjects/Player_Xp.cs';data=(a.source/rel).read_bytes();assert data==urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{rel}',timeout=30).read();source=data.decode('utf-8-sig')
def method(sig):
 start=source.index(sig);op=source.index('{',start);depth=0
 for i in range(op,len(source)):
  depth+=(source[i]=='{')-(source[i]=='}')
  if depth==0:return source[start:i+1]
 raise ValueError(sig)
h=Path(__file__).with_name('earned_harness.cs').read_text()
for marker,sig in [('MAXLEVEL','public static uint GetMaxLevel()'),('UPDATE','private void UpdateXpAndLevel('),('LEVELUP','private void CheckForLevelup()')]:h=h.replace('// '+marker,method(sig))
with tempfile.TemporaryDirectory(prefix='bace-earned-oracle-')as td:
 b=Path(td);(b/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><RollForward>Major</RollForward></PropertyGroup></Project>');(b/'Program.cs').write_text(h);subprocess.run([a.dotnet,'build','--nologo','-o',str(b/'out'),str(b/'Oracle.csproj')],check=True);out=subprocess.check_output([a.dotnet,str(b/'out/Oracle.dll')],text=True)
Path(__file__).parent.parent.joinpath('tests/fixtures/earned.csv').write_text(f'# official ACE {PIN}; sha256 {hashlib.sha256(data).hexdigest()} {rel}\n'+out)
