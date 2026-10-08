#!/usr/bin/env python3
"""Pinned unmodified GetEmoteSet oracle; fixed random sample is a test input."""
from pathlib import Path
import argparse,hashlib,subprocess,tempfile,urllib.request
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
SOURCE='Source/ACE.Server/WorldObjects/Managers/EmoteManager.cs'
def main():
 p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args();root=Path(__file__).resolve().parent
 data=(a.source/SOURCE).read_bytes();official=urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{SOURCE}').read()
 if data!=official:raise ValueError('modified official source')
 text=data.decode();start=text.index('public PropertiesEmote GetEmoteSet(');brace=text.index('{',start);end=brace+1;depth=1
 while depth:
  if text[end]=='{':depth+=1
  elif text[end]=='}':depth-=1
  end+=1
 harness=(root/'native_harness.cs').read_text().replace('/*METHOD*/',text[start:end])
 with tempfile.TemporaryDirectory(prefix='ace-native-emotes-') as t:
  t=Path(t);(t/'Program.cs').write_text(harness);(t/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>')
  subprocess.run([a.dotnet,'build','--nologo','-o',str(t/'out'),str(t/'Oracle.csproj')],check=True)
  result=subprocess.check_output([a.dotnet,str(t/'out/Oracle.dll')],text=True)
 (root.parent/'tests/fixtures/native_selection.csv').write_text('# ACE '+PIN+' AGPL-3.0-only\n# '+SOURCE+' sha256='+hashlib.sha256(data).hexdigest()+'\n'+result)
if __name__=='__main__':main()
