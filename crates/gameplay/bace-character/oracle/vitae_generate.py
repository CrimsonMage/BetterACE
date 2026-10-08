"""Unchanged pinned ACE vitae update/threshold/reduction methods; fake network/timers."""
import argparse,hashlib,subprocess,tempfile,urllib.request
from pathlib import Path
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b';p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args();headers=[]
def read(rel):
 data=(a.source/rel).read_bytes();assert data==urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{rel}',timeout=30).read();headers.append(f'# sha256 {hashlib.sha256(data).hexdigest()} {rel}');return data.decode('utf-8-sig')
xp=read('Source/ACE.Server/WorldObjects/Player_Xp.cs');enchant=read('Source/ACE.Server/WorldObjects/Managers/EnchantmentManager.cs')
def method(s,sig):
 start=s.index(sig);op=s.index('{',start);depth=0
 for i in range(op,len(s)):
  depth+=(s[i]=='{')-(s[i]=='}')
  if depth==0:return s[start:i+1]
 raise ValueError(sig)
h=Path(__file__).with_name('vitae_harness.cs').read_text()
for marker,s,sig in [('MIN',enchant,'public float GetMinVitae('),('REDUCE',enchant,'public virtual float ReduceVitae()'),('THRESHOLD',xp,'private double VitaeCPPoolThreshold('),('UPDATE',xp,'private void UpdateXpVitae(')]:h=h.replace('// '+marker,method(s,sig))
with tempfile.TemporaryDirectory(prefix='bace-vitae-oracle-')as td:
 b=Path(td);(b/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><RollForward>Major</RollForward></PropertyGroup></Project>');(b/'Program.cs').write_text(h);subprocess.run([a.dotnet,'build','--nologo','-o',str(b/'out'),str(b/'Oracle.csproj')],check=True);out=subprocess.check_output([a.dotnet,str(b/'out/Oracle.dll')],text=True)
Path(__file__).parent.parent.joinpath('tests/fixtures/vitae.csv').write_text(f'# official ACE {PIN}\n'+'\n'.join(headers)+'\n'+out)
