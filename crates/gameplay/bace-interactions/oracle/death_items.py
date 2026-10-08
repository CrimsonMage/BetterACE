#!/usr/bin/env python3
"""Compile unchanged pinned ACE DeathItem.cs with explicit float random draws."""
import argparse,pathlib,subprocess,tempfile,hashlib,json
p=argparse.ArgumentParser();p.add_argument('--source',type=pathlib.Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args()
source=a.source/'Source/ACE.Server/Entity/DeathItem.cs';enums=a.source/'Source/ACE.Entity/Enum/ItemType.cs';root=pathlib.Path(__file__).resolve().parents[1]
program='''using System;using System.Linq;using System.Collections.Generic;using ACE.Entity.Enum;using ACE.Server.WorldObjects;using ACE.Server.Entity;
namespace ACE.Server.WorldObjects {public class WorldObject {public int Id;public string Name="Item";public ItemType ItemType;public int? Value,StackSize;}}
namespace ACE.Common {public static class ThreadSafeRandom {public static int index;public static float Next(float min,float max){float[] values={-0.1f,0f,0.1f};return values[index++%3];}}}
class Program {static void Main(){
 string[] cases={"1:1:1000:1;2:1:800:1;3:1:700:1;4:2:300:1;5:2048:1000:10", "1:1:100:1;2:2:100:1;3:4:100:1;4:8:100:1;5:2048:100:1", "1:512:100000:1;2:3:100000:1;3:32:19:2;4:32:17:1;5:32:15:1", "1:32768:100:1;2:2097152:99:1;3:8388608:98:1;4:67108864:97:1;5:134217728:96:1;6:4194304:95:1;7:33554432:94:1", "1:4096:99:1;2:524288:101:1;3:8192:103:1;4:16384:105:1;5:262144:107:1;6:128:109:1", "1:1:-1:1;2:1:-3:1;3:2:0:1;4:8:1:1"};
 foreach(var input in cases){ACE.Common.ThreadSafeRandom.index=0;var rows=input.Split(';').Select(row=>row.Split(':').Select(int.Parse).ToArray()).Select(v=>new WorldObject{Id=v[0],ItemType=(ItemType)v[1],Value=v[2],StackSize=v[3]}).ToList();var sorted=new DeathItems(rows);Console.WriteLine(input+","+string.Join(";",sorted.Inventory.Select(i=>$"{i.WorldObject.Id}:{i.AdjustedValue}")));}
}}
'''
with tempfile.TemporaryDirectory(prefix='bace-death-oracle-') as temp:
 t=pathlib.Path(temp);(t/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>');(t/'DeathItem.cs').write_bytes(source.read_bytes());(t/'ItemType.cs').write_bytes(enums.read_bytes());(t/'Program.cs').write_text(program)
 subprocess.run([a.dotnet,'build','--nologo','-o',str(t/'out'),str(t/'Oracle.csproj')],check=True);output=subprocess.check_output([a.dotnet,str(t/'out/Oracle.dll')],text=True)
 (root/'tests/fixtures/death_items.csv').write_text('# inputs id:type:value:stack,outputs id:adjustedvalue; variance draws repeat -0.1,0,0.1\n'+output)
 (root/'tests/fixtures/death_items.provenance.json').write_text(json.dumps({'repository':'https://github.com/ACEmulator/ACE','commit':'47edade3bd3f6044b676d4eb877c4965c7eda62b','sources':{str(f.relative_to(a.source)):hashlib.sha256(f.read_bytes()).hexdigest() for f in [source,enums]},'scope':'unchanged complete DeathItem.cs sorting/category/float variance implementation; random sequence and WorldObject fields are harness inputs'},indent=2)+'\n')
