#!/usr/bin/env python3
"""Compile original ACE group serializers and unchanged action read prefixes.
Domain collaborators are synthetic; every compiled/extracted source is hashed.
"""
from pathlib import Path
import tempfile, subprocess, hashlib, re
ROOT=Path(__file__).resolve().parents[4]
SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
NETWORK=SRC/'ACE.Server/Network'
def method(text,signature):
 start=text.index(signature);a=text.index('{',start);depth=1;b=a+1
 while depth:
  depth+=(text[b]=='{')-(text[b]=='}');b+=1
 return text[start:b]
files=[NETWORK/'Extensions.cs',NETWORK/'Structure/HashComparer.cs',NETWORK/'Structure/PackableHashTable.cs']
files += [NETWORK/f'Structure/{name}.cs' for name in ['AllegianceHierarchy','AllegianceProfile','AllegianceData','FellowshipLockData']]
files += [NETWORK/f'GameEvent/Events/GameEvent{name}.cs' for name in ['FellowshipFullUpdate','FellowshipUpdateFellow','FellowshipDisband','FellowshipDismiss','FellowshipQuit','FellowshipFellowUpdateDone','AllegianceUpdate','AllegianceInfoResponse','AllegianceAllegianceUpdateDone','ConfirmationRequest','ConfirmationDone']]
files += [NETWORK/'GameEvent/GameEventType.cs',NETWORK/'Enum/AllegianceIndex.cs',SRC/'ACE.Common/Extensions/BinaryReaderExtensions.cs']
files += [SRC/f'ACE.Entity/Enum/{name}.cs' for name in ['Gender','HeritageGroup','AllegianceOfficerLevel','FellowUpdateType','ConfirmationType','AllegianceHouseAction','AllegianceLockAction']]
# These bodies are extracted verbatim from a gameplay-heavy source.
fellow=SRC/'ACE.Server/Entity/Fellowship.cs';files.append(fellow)
body=method(fellow.read_text(),'public static void Write(this BinaryWriter writer, Dictionary<uint, int> departedFellows)')
extra='namespace ACE.Server.Entity { public static class DepartedWriter { static readonly ACE.Server.Network.Structure.HashComparer hashComparer=new(32); '+body+' }}'
reads=[]
for path in sorted((NETWORK/'GameAction/Actions').glob('*.cs')):
 if not any(n in path.name for n in ['Fellowship','Allegiance','Confirmation','Motd']) or 'HouseModify' in path.name:continue
 source=path.read_text();name=re.search(r'GameActionType\.(\w+)',source).group(1)
 lines=[l.strip().split('//')[0] for l in source.splitlines() if 'message.Payload.Read' in l]
 variables=[re.search(r'(?:var|uint|bool)\s+(\w+)\s*=',l).group(1) for l in lines]
 original='\n'.join(lines)
 writes=''.join('w.WriteString16L("Rune");' if 'ReadString16L' in l else 'w.Write(0xffffffffu);' for l in lines)
 reads.append(f'''{{using var stream=new MemoryStream();using(var w=new BinaryWriter(stream,System.Text.Encoding.UTF8,true)){{{writes}}}var bytes=stream.ToArray();var message=new ClientMessage{{Payload=new BinaryReader(new MemoryStream(bytes))}};{original}Console.WriteLine("input,{name},"+Convert.ToHexString(bytes)+","+message.Payload.BaseStream.Position);}}''')
 files.append(path)
harness=(Path(__file__).parent/'group_harness.cs').read_text().replace('INPUT_CASES','\n'.join(reads))
with tempfile.TemporaryDirectory(prefix='group-oracle-') as tmp:
 p=Path(tmp)
 for i,f in enumerate(files):
  if f==fellow or 'GameAction/Actions' in str(f):continue
  (p/f'source{i}.cs').write_bytes(f.read_bytes())
 (p/'Extra.cs').write_text('using System;using System.IO;using System.Collections.Generic;using ACE.Server.Network;using ACE.Server.Network.Structure;\n'+extra)
 (p/'Program.cs').write_text(harness)
 (p/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 subprocess.run(['/tmp/bace-crafting-dotnet/dotnet','build','--nologo','-o',str(p/'build')],cwd=p,check=True)
 output=subprocess.check_output(['/tmp/bace-crafting-dotnet/dotnet',str(p/'build/oracle.dll')],text=True)
 target=Path(__file__).resolve().parents[1]/'tests/fixtures/groups.csv'
 target.write_text(''.join('# '+str(f.relative_to(SRC))+' '+hashlib.sha256(f.read_bytes()).hexdigest()+'\n' for f in files)+output)
 print(target)
