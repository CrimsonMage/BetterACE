#!/usr/bin/env python3
"""Compile unmodified ACE DAT spell/component parsers against synthetic records."""
import argparse,hashlib,json,subprocess,tempfile,urllib.request
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b';BASE='Source/ACE.DatLoader/'
FILES=[BASE+x+'.cs' for x in ['IUnpackable','BinaryReaderExtensions','UnpackableExtensions','DatFileType','DatFileTypeAttribute','DatDatabaseType','DatDatabaseTypeAttribute','DatFileTypeExtensionAttribute','DatFileTypeIdRangeAttribute','FileTypes/FileType','FileTypes/SpellTable','FileTypes/SpellComponentsTable','Entity/SpellBase','Entity/SpellComponentBase','Entity/SpellSet','Entity/SpellSetTiers']]+['Source/ACE.Entity/Enum/'+x+'.cs' for x in ['MagicSchool','SpellCategory','SpellType']]
def main():
 p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',default='dotnet');a=p.parse_args();root=Path(__file__).resolve().parent
 def verified(rel):
  data=(a.source/rel).read_bytes();official=urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{rel}',timeout=30).read()
  if data!=official:raise ValueError(rel)
  return rel,data
 with ThreadPoolExecutor(max_workers=4) as pool:sources=dict(pool.map(verified,FILES))
 harness=(root/'dat_magic_harness.cs').read_bytes()
 with tempfile.TemporaryDirectory(prefix='ace-dat-magic-') as td:
  b=Path(td)
  for i,(rel,data)in enumerate(sources.items()):(b/f'{i}.cs').write_bytes(data)
  (b/'Program.cs').write_bytes(harness);(b/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>')
  subprocess.run([a.dotnet,'build','--nologo','-o',str(b/'out'),str(b/'Oracle.csproj')],check=True)
  vectors=json.loads(subprocess.check_output([a.dotnet,str(b/'out/Oracle.dll')],text=True))
 fixture={'repository':'https://github.com/ACEmulator/ACE','commit':PIN,'source_sha256':{p:hashlib.sha256(d).hexdigest() for p,d in sources.items()},'harness':'Unmodified pinned parsers, signed-byte CP1252 hash, formula decryption/account substitution and sparse set tiers. Synthetic BinaryWriter input only.','vectors':vectors}
 (root.parent/'fixtures/dat_magic.json').write_text(json.dumps(fixture,indent=2)+'\n')
if __name__=='__main__':main()
