#!/usr/bin/env python3
"""Independent ACE component mapper/formula oracle, synthetic records only."""
import argparse,hashlib,struct,subprocess,tempfile,urllib.request
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);p.add_argument('--framework',default='net10.0');a=p.parse_args();pin='47edade3bd3f6044b676d4eb877c4965c7eda62b'
files=['Source/ACE.Server/Entity/SpellFormula.cs','Source/ACE.DatLoader/FileTypes/DualDidMapper.cs','Source/ACE.DatLoader/BinaryReaderExtensions.cs','Source/ACE.Server/Managers/RecipeManager.cs'];sources={}
for name in files:
 data=(a.source/name).read_bytes();assert data==urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{pin}/{name}',timeout=30).read();sources[name]=data

def block(s,signature):
 start=s.index(signature);opening=s.index('{',start);depth=0
 for end in range(opening,len(s)):
  depth+=(s[end]=='{')-(s[end]=='}')
  if depth==0:return s[start:end+1]
 raise ValueError(signature)
f,m,r,recipe=[sources[n].decode('utf-8-sig') for n in files]
foci=block(f,'public enum Scarab')+'\nclass Formula { public List<uint> Components=new(); public List<uint> FociFormula=new();\n'+block(f,'public static Dictionary<Scarab, uint> ScarabPower')+';\n'+block(f,'public static bool IsScarab(')+'\n'+block(f,'public uint Power')+'\n'+block(f,'public List<uint> GetFociFormula()')+'\n}'
mapper='class Mapper {public uint Id; public byte ClientIDNumberingType,ClientNameNumberingType,ServerIDNumberingType,ServerNameNumberingType; public Dictionary<uint,uint> ClientEnumToID=new(),ServerEnumToID=new(); public Dictionary<uint,string> ClientEnumToName=new(),ServerEnumToName=new();\n'+block(m,'public override void Unpack(').replace('override ','').replace('(NumberingType)','(byte)')+'\n}'
extensions='static class Extensions {\n'+block(r,'public static uint ReadCompressedUInt32(')+'\n'+block(r,'public static string ReadPString(')+'\n}'
payload=bytearray(struct.pack('<I',0x27000002));payload+=bytes([3,2]);payload+=struct.pack('<IIII',1,600,188,601);payload+=bytes([0,2]);
for key,name in [(1,b'Lead'),(188,b'Taper')]:payload+=struct.pack('<I',key)+bytes([len(name)])+name
payload+=bytes([7,1])+struct.pack('<II',1,0xffffffff)+bytes([1,0])
material_source='enum MaterialType:uint {} class QuietLog {public void Error(string message) {}} class PortalArchive {public Mapper Current;public T ReadFromDat<T>(uint id)=>(T)(object)Current;} static class DatManager {public static PortalArchive PortalDat=new();} static class Recipe {static QuietLog log=new(); const uint MaterialDualDID=0x27000000;'+block(recipe,'public static string GetMaterialName(')+'}'
material=bytearray(struct.pack('<I',0x27000000))+bytes([0,0,0,2])
for key,name in [(1,b'Green_Garnet'),(2,b'Iron')]:material+=struct.pack('<I',key)+bytes([len(name)])+name
material+=bytes([0,0,0,0])
program='using DualDidMapper=Mapper;using System;using System.IO;using System.Collections.Generic;using System.Linq;\n'+foci+'\n'+mapper+'\n'+extensions+'\n'+material_source+'''
class Program {static void Main(string[] args){if(args[0]=="mapper"){var m=new Mapper();using var r=new BinaryReader(File.OpenRead(args[1]));m.Unpack(r);Console.WriteLine($"numbering,{m.ClientIDNumberingType},{m.ClientNameNumberingType},{m.ServerIDNumberingType},{m.ServerNameNumberingType}");foreach(var p in m.ClientEnumToID)Console.WriteLine($"client,{p.Key},{p.Value}");foreach(var p in m.ClientEnumToName)Console.WriteLine($"name,{p.Key},{p.Value}");foreach(var p in m.ServerEnumToID)Console.WriteLine($"server,{p.Key},{p.Value}");Console.WriteLine($"length,{r.BaseStream.Position}");if(m.Id==0x27000000){DatManager.PortalDat.Current=m;foreach(var key in m.ClientEnumToName.Keys)Console.WriteLine($"material,{key},{Recipe.GetMaterialName((MaterialType)key)}");}}else{foreach(uint first in new uint[]{0,1,2,3,4,5,6,110,112,192,193}){var f=new Formula{Components=new(){first,111,112,27}};Console.WriteLine($"{string.Join(';',f.Components)}={string.Join(';',f.GetFociFormula())}");}}}}
'''
with tempfile.TemporaryDirectory(prefix='ace-components-') as td:
 root=Path(td);(root/'Program.cs').write_text(program);(root/'record.bin').write_bytes(payload);(root/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>'+a.framework+'</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>');subprocess.run([a.dotnet,'build',str(root/'oracle.csproj'),'-o',str(root/'out'),'--nologo','-v:q'],check=True)
 formula=subprocess.check_output([a.dotnet,str(root/'out/oracle.dll'),'foci'],text=True);mapping=subprocess.check_output([a.dotnet,str(root/'out/oracle.dll'),'mapper',str(root/'record.bin')],text=True)
 (root/'material.bin').write_bytes(material);material_mapping=subprocess.check_output([a.dotnet,str(root/'out/oracle.dll'),'mapper',str(root/'material.bin')],text=True)
header=f'# ACE {pin}; AGPL-3.0-only ACE contributors\n'+''.join(f'# sha256 {hashlib.sha256(data).hexdigest()} {name}\n' for name,data in sources.items())
base=Path(__file__).resolve().parent.parent
(base/'tests/fixtures/foci.csv').write_text(header+formula)
dat=base.parents[1]/'assets/bace-dat/tests/fixtures';dat.mkdir(parents=True,exist_ok=True)
(dat/'components_mapper.hex').write_text(payload.hex()+'\n');(dat/'components_mapper.csv').write_text(header+mapping)

(dat/'materials_mapper.hex').write_text(material.hex()+'\n');(dat/'materials_mapper.csv').write_text(header+material_mapping)
