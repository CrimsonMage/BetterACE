#!/usr/bin/env python3
"""Verbatim ACE appearance palette, attribute formula and heritage factory oracle."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile
import urllib.request
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',default='dotnet');a=p.parse_args()
def method(s,signature):
    start=s.index(signature);opening=s.index('{',start);depth=0
    for i in range(opening,len(s)):
        depth+=(s[i]=='{')-(s[i]=='}')
        if depth==0:return s[start:i+1]
    raise ValueError(signature)
headers=[f'# official ACE {PIN}; synthetic adapters; AGPL-3.0-only']
files={}
for rel in ['Source/ACE.Server/Factories/PlayerFactory.cs','Source/ACE.DatLoader/FileTypes/PaletteSet.cs','Source/ACE.Server/Entity/AttributeFormula.cs','Source/ACE.Common/Extensions/FloatExtensions.cs','Source/ACE.Entity/Enum/HeritageGroup.cs','Source/ACE.Entity/Enum/WeaponType.cs','Source/ACE.Entity/Enum/Properties/PropertyAttribute.cs']:
    data=(a.source/rel).read_bytes();official=urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{rel}',timeout=30).read()
    if data!=official:raise ValueError(f'Wrong source {rel}')
    headers.append(f'# sha256 {hashlib.sha256(data).hexdigest()} {rel}');files[Path(rel).name]=data.decode('utf-8-sig')
harness=r'''
using System;using System.Collections.Generic;using System.Globalization;
// ENUMS
namespace DatLoader.Entity {class SkillFormula {public uint X=1,Z,Attr1=2,Attr2=6;}}
class Attribute {public uint Current,Base;}
class Creature {public Dictionary<PropertyAttribute,Attribute> Attributes=new();}
class Player {
 public HeritageGroup HeritageGroup;
 public int AugmentationJackOfAllTrades,AugmentationCriticalExpertise,AugmentationDamageReduction,AugmentationCriticalDefense,AugmentationInfusedLifeMagic,AugmentationCriticalPower,AugmentationIncreasedCarryingCapacity;
}
static class FloatExtensions {// ROUND
}
class Program {
 public List<uint> PaletteList=new();
 // METHODS
 static void Main(){
 CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
 foreach(int count in new[]{1,2,3,7,256})foreach(double hue in new[]{0,0.1,0.5,0.999999,1}){
  var palette=new Program();for(uint i=0;i<count;i++)palette.PaletteList.Add(0x04000000+i);
  Console.WriteLine($"palette,{count},{hue:R},{palette.GetPaletteID(hue)}");
 }
 foreach(uint first in new uint[]{10,11,33,55,99,100})foreach(uint second in new uint[]{10,11,99})foreach(uint divisor in new uint[]{1,2,3,4}){
  var c=new Creature();c.Attributes[PropertyAttribute.Endurance]=new Attribute{Current=first,Base=first};c.Attributes[PropertyAttribute.Self]=new Attribute{Current=second,Base=second};
  Console.WriteLine($"vital,{first},{second},{divisor},{GetFormula(c,new DatLoader.Entity.SkillFormula{Z=divisor},false)}");
 }
 for(int heritage=1;heritage<=11;heritage++){
  GetMasteries((HeritageGroup)heritage,out var melee,out var ranged);var player=new Player{HeritageGroup=(HeritageGroup)heritage};SetInnateAugmentations(player);
  Console.WriteLine($"heritage,{heritage},{(int)melee},{(int)ranged},{player.AugmentationJackOfAllTrades},{player.AugmentationCriticalExpertise},{player.AugmentationDamageReduction},{player.AugmentationCriticalDefense},{player.AugmentationInfusedLifeMagic},{player.AugmentationCriticalPower},{player.AugmentationIncreasedCarryingCapacity}");
 }
 }
}
'''
enums='\n'.join(method(files[f],sig) for f,sig in [('HeritageGroup.cs','public enum HeritageGroup'),('WeaponType.cs','public enum WeaponType'),('PropertyAttribute.cs','public enum PropertyAttribute : ushort')])
methods='\n'.join([method(files['PaletteSet.cs'],'public uint GetPaletteID('),method(files['AttributeFormula.cs'],'public static uint GetFormula(Creature creature, DatLoader.Entity.SkillFormula'),method(files['PlayerFactory.cs'],'private static void GetMasteries('),method(files['PlayerFactory.cs'],'private static void SetInnateAugmentations(')])
harness=harness.replace('// ENUMS',enums).replace('// METHODS',methods).replace('// ROUND',method(files['FloatExtensions.cs'],'public static int Round(this float'))
with tempfile.TemporaryDirectory(prefix='bace-factory-oracle-') as td:
 b=Path(td);(b/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>');(b/'Program.cs').write_text(harness)
 subprocess.run([a.dotnet,'build','--nologo','-o',str(b/'out'),str(b/'Oracle.csproj')],check=True)
 lines=subprocess.check_output([a.dotnet,str(b/'out/Oracle.dll')],text=True).splitlines()
 out=Path(__file__).resolve().parents[1]/'tests/fixtures/factory.csv';out.write_text('\n'.join(headers+lines)+'\n')
