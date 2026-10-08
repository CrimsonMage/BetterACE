#!/usr/bin/env python3
"""Verify synthetic taboo fixture through unmodified pinned ACE Unpack methods."""
import argparse,hashlib,subprocess,tempfile,urllib.request
from pathlib import Path
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args()
def method(s,signature):
 start=s.index(signature);opening=s.index('{',start);depth=0
 for i in range(opening,len(s)):
  depth+=(s[i]=='{')-(s[i]=='}')
  if depth==0:return s[start:i+1]
 raise ValueError(signature)
files={};headers=[f'# official ACE {PIN}; synthetic taboo decoder and matching fixture']
for rel in ['Source/ACE.DatLoader/FileTypes/TabooTable.cs','Source/ACE.DatLoader/Entity/TabooTableEntry.cs','Source/ACE.DatLoader/UnpackableExtensions.cs']:
 data=(a.source/rel).read_bytes()
 if data!=urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{rel}',timeout=30).read():raise ValueError(rel)
 files[Path(rel).name]=data.decode('utf-8-sig');headers.append(f'# sha256 {hashlib.sha256(data).hexdigest()} {rel}')
source='''using System;using System.IO;using System.Collections.Generic;using System.Text.RegularExpressions;
interface IUnpackable{void Unpack(BinaryReader r);} abstract class FileType{public uint Id;public abstract void Unpack(BinaryReader r);}
static class Extensions{// DICTIONARY
}
class TabooTableEntry:IUnpackable {public uint Unknown1;public ushort Unknown2;public List<string> BannedPatterns=new();
// ENTRY
}
class TabooTable:FileType {public Dictionary<uint,TabooTableEntry> TabooTableEntries=new();
// TABLE
}
class Program{static void Main(){
 using var m=new MemoryStream();using var w=new BinaryWriter(m);w.Write(0x0e00001eu);w.Write((byte)1);w.Write((byte)2);
 w.Write(1u);w.Write(0x10101u);w.Write((ushort)0);w.Write(2u);w.Write("foo");w.Write("*bar*");
 w.Write(4u);w.Write(0x10101u);w.Write((ushort)0);w.Write(1u);w.Write("baz*");w.Flush();
 var bytes=m.ToArray();Console.WriteLine("bytes,"+Convert.ToHexString(bytes));m.Position=0;var table=new TabooTable();table.Unpack(new BinaryReader(m));
 foreach(var e in table.TabooTableEntries)Console.WriteLine($"entry,{e.Key},{e.Value.Unknown1},{e.Value.Unknown2},{string.Join(';',e.Value.BannedPatterns)}");
 foreach(var name in new[]{"Foo","Food","Barbarian","Alice foo","Bazooka","Alice"})Console.WriteLine($"match,{name},{(table.ContainsBadWord(name)?1:0)}");
}}
'''
source=source.replace('// DICTIONARY',method(files['UnpackableExtensions.cs'],'public static void Unpack<T>(this Dictionary<uint, T> value, BinaryReader reader, uint fixedQuantity)'))
source=source.replace('// ENTRY',method(files['TabooTableEntry.cs'],'public void Unpack(')+'\n'+method(files['TabooTableEntry.cs'],'public bool ContainsBadWord('))
source=source.replace('// TABLE',method(files['TabooTable.cs'],'public override void Unpack(')+'\n'+method(files['TabooTable.cs'],'public bool ContainsBadWord('))
with tempfile.TemporaryDirectory(prefix='bace-taboo-oracle-') as td:
 b=Path(td);(b/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>');(b/'Program.cs').write_text(source)
 subprocess.run([a.dotnet,'build','--nologo','-o',str(b/'out'),str(b/'Oracle.csproj')],check=True)
 output=subprocess.check_output([a.dotnet,str(b/'out/Oracle.dll')],text=True)
(Path(__file__).parent.parent/'tests/fixtures/taboo.csv').write_text('\n'.join(headers)+'\n'+output)
