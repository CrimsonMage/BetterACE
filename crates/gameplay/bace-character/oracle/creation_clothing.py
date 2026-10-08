#!/usr/bin/env python3
"""Original ACE clothing overlap methods; synthetic items, no full PlayerFactory claim."""
import argparse,pathlib,tempfile,subprocess,urllib.request,hashlib
p=argparse.ArgumentParser();p.add_argument('--source',required=True);p.add_argument('--dotnet',default='dotnet');a=p.parse_args();pin='47edade3bd3f6044b676d4eb877c4965c7eda62b';rel='Source/ACE.Server/WorldObjects/Creature_Equipment.cs';raw=(pathlib.Path(a.source)/rel).read_bytes();assert raw==urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{pin}/{rel}').read();source=raw.decode()
def method(signature):
 start=source.index(signature);i=source.index('{',start)+1;depth=1
 while depth:depth+=(source[i]=='{')-(source[i]=='}');i+=1
 return source[start:i]
methods='\n'.join(method(s) for s in ['public bool WieldedLocationIsAvailable(', 'public List<WorldObject> GetEquippedClothingArmor(', 'public List<WorldObject> GetEquippedItems('])
program='''using System;using System.Linq;using System.Collections.Generic;
[Flags] enum EquipMask:uint{MissileAmmo=0x800000} [Flags] enum CoverageMask:uint{None=0}
class WorldObject{public CoverageMask? ClothingPriority;public EquipMask? CurrentWieldedLocation;public int? ParentLocation;}
class Clothing:WorldObject{}
class Creature{public Dictionary<int,WorldObject> EquippedObjects=new();bool IsWeaponSlot(EquipMask mask)=>false;void GetPlacementLocation(WorldObject w,EquipMask m,out int a,out int b){a=0;b=0;}
'''+methods+'''}
class Program{static void Main(){foreach(int existing in new[]{-1,0,1,2,3,16})foreach(int candidate in new[]{-1,0,1,2,3,32})foreach(uint wield in new uint[]{4,64}){var c=new Creature();c.EquippedObjects[1]=new Clothing{ClothingPriority=existing<0?null:(CoverageMask)existing,CurrentWieldedLocation=(EquipMask)4};var item=new Clothing{ClothingPriority=candidate<0?null:(CoverageMask)candidate};Console.WriteLine($"{existing},{candidate},{wield},{(c.WieldedLocationIsAvailable(item,(EquipMask)wield)?1:0)}");}}}'''
with tempfile.TemporaryDirectory() as tmp:
 d=pathlib.Path(tmp);(d/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><WarningLevel>0</WarningLevel></PropertyGroup></Project>');(d/'Program.cs').write_text(program);subprocess.run([a.dotnet,'build',str(d/'oracle.csproj'),'-o',str(d/'out'),'--nologo'],check=True,stdout=subprocess.DEVNULL);output=subprocess.check_output([a.dotnet,str(d/'out/oracle.dll')],text=True)
 (pathlib.Path(__file__).parent.parent/'tests/fixtures/creation_clothing.csv').write_text(f'# ACE {pin} {rel} SHA256 {hashlib.sha256(raw).hexdigest()}\n'+output)
