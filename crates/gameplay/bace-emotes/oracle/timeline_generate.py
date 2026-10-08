"""Compile unchanged pinned ACE scheduling methods; timing stubs only."""
import argparse,hashlib,subprocess,tempfile,urllib.request
from pathlib import Path
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args()
rel='Source/ACE.Server/WorldObjects/Managers/EmoteManager.cs';data=(a.source/rel).read_bytes()
assert data==urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{rel}',timeout=30).read()
s=data.decode('utf-8-sig')
def method(signature):
 start=s.index(signature);opening=s.index('{',start);depth=0
 for i in range(opening,len(s)):
  depth+=(s[i]=='{')-(s[i]=='}')
  if depth==0:return s[start:i+1]
 raise ValueError(signature)
h=Path(__file__).with_name('timeline_harness.cs').read_text()
for marker,sig in [('EXECUTE','public bool ExecuteEmoteSet(PropertiesEmote emoteSet'),('ENQUEUE','public void Enqueue(PropertiesEmote emoteSet'),('DO_ENQUEUE','private void DoEnqueue(PropertiesEmote emoteSet')]:h=h.replace('// '+marker,method(sig))
with tempfile.TemporaryDirectory(prefix='bace-emote-timing-')as t:
 b=Path(t);(b/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><RollForward>Major</RollForward></PropertyGroup></Project>');(b/'Program.cs').write_text(h)
 subprocess.run([a.dotnet,'build','--nologo','-o',str(b/'out'),str(b/'Oracle.csproj')],check=True)
 output=subprocess.check_output([a.dotnet,str(b/'out/Oracle.dll')],text=True)
header=f'# official ACE {PIN}; unchanged scheduling methods with fake explicit clock\n# sha256 {hashlib.sha256(data).hexdigest()} {rel}\n'
Path(__file__).parent.parent.joinpath('tests/fixtures/native_timeline.csv').write_text(header+'\n'.join(l for l in output.splitlines() if l.startswith('basic:'))+'\n')
Path(__file__).parent.parent.joinpath('tests/fixtures/native_immediate.csv').write_text(header+'\n'.join(l for l in output.splitlines() if not l.startswith('basic:'))+'\n')
