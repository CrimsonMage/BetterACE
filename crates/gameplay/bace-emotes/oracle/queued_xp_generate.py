"""Unchanged pinned EarnXP/GrantXP/recipient and NPC enqueue methods composed."""
import argparse,hashlib,subprocess,tempfile,urllib.request
from pathlib import Path
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b';p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args();headers=[]
def read(rel):
 data=(a.source/rel).read_bytes();assert data==urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{rel}',timeout=30).read();headers.append(f'# sha256 {hashlib.sha256(data).hexdigest()} {rel}');return data.decode('utf-8-sig')
xp=read('Source/ACE.Server/WorldObjects/Player_Xp.cs');emote=read('Source/ACE.Server/WorldObjects/Managers/EmoteManager.cs')
def method(s,sig):
 start=s.index(sig);op=s.index('{',start);depth=0
 for i in range(op,len(s)):
  depth+=(s[i]=='{')-(s[i]=='}')
  if depth==0:return s[start:i+1]
 raise ValueError(sig)
h=Path(__file__).with_name('queued_xp_harness.cs').read_text()
for marker,source,sig in [('MAXLEVEL',xp,'public static uint GetMaxLevel()'),('UPDATE',xp,'private void UpdateXpAndLevel('),('LEVELUP',xp,'private void CheckForLevelup()'),('EARN',xp,'public void EarnXP('),('GRANT',xp,'public void GrantXP('),('ITEMXP',xp,'public void GrantItemXP(long'),('EXECUTE',emote,'public bool ExecuteEmoteSet(PropertiesEmote emoteSet'),('ENQUEUE',emote,'public void Enqueue(PropertiesEmote emoteSet'),('DO_ENQUEUE',emote,'private void DoEnqueue(PropertiesEmote emoteSet')]:h=h.replace('// '+marker,method(source,sig))
with tempfile.TemporaryDirectory(prefix='bace-queued-xp-')as td:
 b=Path(td);(b/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><RollForward>Major</RollForward></PropertyGroup></Project>');(b/'Program.cs').write_text(h);subprocess.run([a.dotnet,'build','--nologo','-o',str(b/'out'),str(b/'Oracle.csproj')],check=True);output=subprocess.check_output([a.dotnet,str(b/'out/Oracle.dll')],text=True)
Path(__file__).parent.parent.joinpath('tests/fixtures/queued_xp.csv').write_text(f'# official ACE {PIN}; source methods unchanged, explicit synthetic owner queue\n'+'\n'.join(headers)+'\n'+output)
