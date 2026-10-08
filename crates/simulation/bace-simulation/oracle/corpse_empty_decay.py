#!/usr/bin/env python3
"""Execute the original empty-corpse branch before ordinary elapsed-time decay.
Only the actual source excerpt runs; synthetic property storage supplies inputs.
"""
from pathlib import Path
import hashlib, subprocess, tempfile
root = Path(__file__).resolve().parents[4]
pin = '47edade3bd3f6044b676d4eb877c4965c7eda62b'
source = root / '.reference' / ('ACE-' + pin) / 'Source/ACE.Server/WorldObjects/WorldObject_Decay.cs'
text = source.read_text()
start = text.index('            var corpse = this as Corpse;')
end = text.index('            if (TimeToRot > 0)', start)
branch = text[start:end]
program = '''using System;using System.Collections.Generic;
static class PropertyManager{public static (bool Item,int Other) GetBool(string s)=>(false,0);}
class WorldObject{public double? TimeToRot;public int? Level;public string Name="Corpse";public uint Guid=1;static class log{public static void InfoFormat(string s,params object[] args){}}
public void EmptyBranch(){var previousTTR=TimeToRot;var elapsed=TimeSpan.FromSeconds(5);BRANCH}}
class Corpse:WorldObject{public const double EmptyDecayTime=15.0;public bool InventoryLoaded=true;public List<int> Inventory=new();}
class Program{static void Main(){foreach(int count in new[]{0,1})foreach(double time in new[]{0,14,15,15.5,16,3600}){var c=new Corpse{TimeToRot=time};for(int i=0;i<count;i++)c.Inventory.Add(i);c.EmptyBranch();Console.WriteLine($"{count}|{time*30}|{c.TimeToRot*30}");}}}
'''.replace('BRANCH', branch)
with tempfile.TemporaryDirectory(prefix='bace-corpse-empty-') as tmp:
    d = Path(tmp)
    (d/'Program.cs').write_text(program)
    (d/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><WarningLevel>0</WarningLevel></PropertyGroup></Project>')
    dotnet = '/tmp/bace-crafting-dotnet/dotnet'
    subprocess.run([dotnet,'build','--nologo','-o',str(d/'build')],cwd=d,check=True)
    output = subprocess.check_output([dotnet,str(d/'build/oracle.dll')],text=True)
    destination = Path(__file__).resolve().parents[1]/'tests/fixtures/corpse_empty_decay.trace'
    destination.write_text('# official ACE '+pin+'\n# WorldObject_Decay.cs sha256='+hashlib.sha256(source.read_bytes()).hexdigest()+'\n'+output)
    print(len(output.splitlines()), 'original empty-corpse branch vectors')
