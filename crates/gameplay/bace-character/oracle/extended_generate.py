#!/usr/bin/env python3
"""Compile unchanged official luminance/item-XP sources and CalcProcRate body."""
import argparse,hashlib,subprocess,tempfile,urllib.request
from pathlib import Path
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
FILES=['Source/ACE.Server/WorldObjects/Player_Luminance.cs','Source/ACE.Server/Entity/ExperienceSystem.cs','Source/ACE.Server/Entity/Aetheria.cs','Source/ACE.Entity/Enum/ItemXpStyle.cs']
def method(text,signature):
 start=text.index(signature);brace=text.index('{',start);level=1;end=brace+1
 while level:
  if text[end]=='{':level+=1
  elif text[end]=='}':level-=1
  end+=1
 return text[start:end]
def main():
 p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args();root=Path(__file__).resolve().parent
 sources={}
 for path in FILES:
  local=(a.source/path).read_bytes();official=urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{path}').read()
  if local!=official:raise ValueError(path)
  sources[path]=local
 with tempfile.TemporaryDirectory(prefix='ace-progression-extended-') as t:
  t=Path(t)
  for path,data in sources.items():
   if not path.endswith('Aetheria.cs'):(t/Path(path).name).write_bytes(data)
  harness=(root/'extended_harness.cs').read_text().replace('/*PROC_METHOD*/',method(sources[FILES[2]].decode(),'public static float CalcProcRate('))
  (t/'Program.cs').write_text(harness);(t/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>')
  subprocess.run([a.dotnet,'build','--nologo','-o',str(t/'out'),str(t/'Oracle.csproj')],check=True)
  result=subprocess.check_output([a.dotnet,str(t/'out/Oracle.dll')],text=True)
 fixture='# ACEmulator/ACE '+PIN+' AGPL-3.0-only\n'+''.join('# '+p+' sha256='+hashlib.sha256(d).hexdigest()+'\n' for p,d in sources.items())+result
 (root.parent/'tests/fixtures/extended.csv').write_text(fixture)
if __name__=='__main__':main()
