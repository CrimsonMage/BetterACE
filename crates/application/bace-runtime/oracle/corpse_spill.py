#!/usr/bin/env python3
"""Original source spill height, resting placement and destruction delay statements."""
from pathlib import Path
import hashlib, subprocess, tempfile
root=Path(__file__).resolve().parents[4]
pin='47edade3bd3f6044b676d4eb877c4965c7eda62b'
path=root/'.reference'/('ACE-'+pin)/'Source/ACE.Server/WorldObjects/WorldObject_Decay.cs'
source=path.read_text()
lines=[next(l.strip() for l in source.splitlines() if token in l) for token in ['item.Location.PositionZ +=','item.Placement = ACE.Entity.Enum.Placement.Resting;','actionChain.AddDelaySeconds(1.0f);']]
program='''using System;
namespace ACE.Entity.Enum{enum Placement{Resting=101}}
class Position{public float PositionZ;}
class Item{public Position Location=new();public float? ObjScale;public ACE.Entity.Enum.Placement Placement;}
class Chain{public float Delay;public void AddDelaySeconds(float value){Delay=value;}}
class Program{static void Main(){foreach(float z in new[]{-5f,0f,1f,100f})foreach(float? scale in new float?[]{null,0.1f,0.4f,1f,2f,10f}){var item=new Item{Location=new Position{PositionZ=z},ObjScale=scale};var actionChain=new Chain();STATEMENTS Console.WriteLine($"{z}|{(scale.HasValue?scale.Value.ToString(\"R\"): \"-\")}|{BitConverter.SingleToInt32Bits(item.Location.PositionZ):X8}|{(int)item.Placement}|{actionChain.Delay}");}}}
'''.replace('STATEMENTS','\n'.join(lines))
with tempfile.TemporaryDirectory(prefix='bace-corpse-spill-') as tmp:
    d=Path(tmp);(d/'Program.cs').write_text(program)
    (d/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><WarningLevel>0</WarningLevel></PropertyGroup></Project>')
    dotnet='/tmp/bace-crafting-dotnet/dotnet';subprocess.run([dotnet,'build','--nologo','-o',str(d/'build')],cwd=d,check=True)
    output=subprocess.check_output([dotnet,str(d/'build/oracle.dll')],text=True)
    (Path(__file__).resolve().parents[1]/'tests/fixtures/corpse_spill.trace').write_text('# ACE '+pin+'\n# WorldObject_Decay.cs sha256='+hashlib.sha256(path.read_bytes()).hexdigest()+'\n'+output)
    print(len(output.splitlines()),'original source spill statement vectors')
