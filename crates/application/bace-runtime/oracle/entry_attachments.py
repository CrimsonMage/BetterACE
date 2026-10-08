#!/usr/bin/env python3
"""Compile original GetPlacementLocation; fixtures include every weapon slot/style."""
from pathlib import Path
import subprocess,tempfile,hashlib
ROOT=Path(__file__).resolve().parents[4];SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
f=SRC/'ACE.Server/WorldObjects/Creature_Equipment.cs';s=f.read_text();a=s.index('private static void GetPlacementLocation(');i=s.index('{',a)+1;depth=1
while depth:depth+=(s[i]=='{')-(s[i]=='}');i+=1
method=s[a:i]
h='''using System;using ACE.Entity.Enum;class WorldObject{public ItemType ItemType;public CombatStyle DefaultCombatStyle;}class Program{METHOD static void Main(){foreach(uint loc in new uint[]{1,0x100000,0x200000,0x400000,0x800000,0x1000000,0x2000000})foreach(int type in new[]{2,4,16})foreach(int style in new[]{0,16,32,64,128,512,1024}){GetPlacementLocation(new WorldObject{ItemType=(ItemType)type,DefaultCombatStyle=(CombatStyle)style},(EquipMask)loc,out var placement,out var parent);Console.WriteLine($"{loc},{type},{style},{(uint)placement},{(uint)parent}");}}}'''.replace('METHOD',method)
with tempfile.TemporaryDirectory(prefix='entry-attachments-')as tmp:
 p=Path(tmp);(p/'Program.cs').write_text(h);files=[f]
 for name in ['ItemType','CombatStyle','Placement','ParentLocation','EquipMask']:
  source=SRC/('ACE.Entity/Enum/'+name+'.cs');files.append(source);(p/(name+'.cs')).write_bytes(source.read_bytes())
 (p/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
 subprocess.run(['/tmp/bace-crafting-dotnet/dotnet','build','--nologo','-o',str(p/'build')],cwd=p,check=True,stdout=subprocess.DEVNULL)
 out=subprocess.check_output(['/tmp/bace-crafting-dotnet/dotnet',str(p/'build/oracle.dll')],text=True)
 Path(__file__).parents[1].joinpath('tests/fixtures/entry_attachments.csv').write_text(''.join('# '+str(v.relative_to(SRC))+' '+hashlib.sha256(v.read_bytes()).hexdigest()+'\n' for v in files)+out)
