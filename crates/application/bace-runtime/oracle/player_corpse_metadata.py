#!/usr/bin/env python3
"""Execute pinned Corpse.RecalculateDecayTime, constructor timestamp assignment,
PK corpse assignment and post-loot Value removal. Wrappers supply only clocks,
property storage and inventory counts. No wire/loot-selection parity is claimed.
"""
from pathlib import Path
import hashlib, subprocess, tempfile
ROOT=Path(__file__).resolve().parents[4]
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
base=ROOT/'.reference'/('ACE-'+PIN)/'Source/ACE.Server/WorldObjects'
paths=[base/'Corpse.cs',base/'Creature_Death.cs',base/'WorldObject.cs']
texts=[p.read_text() for p in paths]
a=texts[0].index('        public void RecalculateDecayTime(Player player)')
b=texts[0].index('\n        /// <summary>',a)
method=texts[0][a:b]
assignment=next(l.strip() for l in texts[2].splitlines() if 'CreationTimestamp = (int)Time.GetUnixTime();' in l)
remove=next(l.strip() for l in texts[1].splitlines() if 'corpse.RemoveProperty(PropertyInt.Value);' in l)
pk=next(l.strip() for l in texts[1].splitlines() if 'corpse.PkLevel = PKLevel.PK;' in l)
program='''using System;using System.Collections.Generic;
enum PropertyInt{Value} enum PKLevel{NPK,PK,PKLite,Free}
static class Time{public static double GetUnixTime()=>1728000000;public static DateTime GetDateTimeFromTimestamp(int t)=>DateTime.UnixEpoch.AddSeconds(t);}
class Player{public int? Level;public string Name="Alice";}
class Corpse{public const double EmptyDecayTime=15.0;public List<int> Inventory=new();public bool InventoryLoaded=true;public double? TimeToRot;public int? Level,CreationTimestamp;public int? Value=100;public PKLevel PkLevel;public string Name="Corpse of Alice";public uint Guid=0x80000001;public static class log{public static void Info(string s){}}
public Corpse(){TIMESTAMP_STATEMENT}
public void RemoveProperty(PropertyInt p){Value=null;}
METHOD
}
class Program{static void Main(){foreach(int level in new[]{1,11,12,100,275})foreach(bool empty in new[]{false,true})foreach(bool isPK in new[]{false,true}){var corpse=new Corpse();if(!empty)corpse.Inventory.Add(1);corpse.RecalculateDecayTime(new Player{Level=level});if(isPK){PKASSIGN}REMOVE Console.WriteLine($"{level}|{empty}|{isPK}|{corpse.Level}|{corpse.TimeToRot}|{corpse.CreationTimestamp}|{(uint)corpse.PkLevel}|{corpse.Value?.ToString()??"-"}");}}}
'''.replace('TIMESTAMP_STATEMENT',assignment).replace('METHOD',method).replace('PKASSIGN',pk).replace('REMOVE',remove)
with tempfile.TemporaryDirectory(prefix='bace-corpse-metadata-') as tmp:
 d=Path(tmp);(d/'Program.cs').write_text(program);(d/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><WarningLevel>0</WarningLevel></PropertyGroup></Project>')
 dotnet='/tmp/bace-crafting-dotnet/dotnet';subprocess.run([dotnet,'build','--nologo','-o',str(d/'build')],cwd=d,check=True)
 out=subprocess.check_output([dotnet,str(d/'build/oracle.dll')],text=True)
 dest=Path(__file__).resolve().parents[1]/'tests/fixtures/player_corpse_metadata.trace'
 dest.write_text('# official ACE '+PIN+'\n'+''.join('# '+p.name+' sha256='+hashlib.sha256(p.read_bytes()).hexdigest()+'\n' for p in paths)+out)
 print(len(out.splitlines()),'original-source corpse metadata rows')
