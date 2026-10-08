#!/usr/bin/env python3
"""Run unchanged pinned ACE per-object spawn loop with controlled placement results.
Upstream ACEmulator contributors, AGPL-3.0; source hashes accompany vectors.
"""
from pathlib import Path
import hashlib
import subprocess
import tempfile
ROOT = Path(__file__).resolve().parents[4]
SOURCE = ROOT / '.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
profile = SOURCE / 'ACE.Server/Entity/GeneratorProfile.cs'
enum = SOURCE / 'ACE.Entity/Enum/RegenLocationType.cs'
source = profile.read_text()
start = source.index('            var spawned = new List<WorldObject>();')
end = source.index('            return spawned;', start) + len('            return spawned;')
with tempfile.TemporaryDirectory(prefix='bace-generator-spawn-') as directory:
    path = Path(directory)
    (path / enum.name).write_text(enum.read_text())
    (path / 'Program.cs').write_text('''
using System; using System.Linq; using System.Collections.Generic; using ACE.Entity.Enum;
class GuidStub { public uint Full; }
class Position { public string ToLOCString() => ""; }
class WorldObject {
    public WorldObject Generator; public uint GeneratorId; public GuidStub Guid=new();
    public uint WeenieClassId=100; public string Name="object"; public Position Location;
    public bool Success, Destroyed; public void Destroy() { Destroyed=true; }
}
class Log { public bool IsDebugEnabled=false; public void Debug(string s) {} }
class Program {
    WorldObject Generator=new(); RegenLocationType RegenLocationType; bool FirstSpawn;
    Log log=new(); int LinkId=0;
    bool Spawn_Specific(WorldObject o)=>o.Success; bool Spawn_Scatter(WorldObject o)=>o.Success;
    bool Spawn_Container(WorldObject o)=>o.Success; bool Spawn_Shop(WorldObject o)=>o.Success;
    bool Spawn_Default(WorldObject o)=>o.Success;
    List<WorldObject> Spawn(List<WorldObject> objects) {''' + source[start:end] + '''}
    static void Main() {
        foreach (uint flags in new uint[]{64,66,68,72,96}) foreach (bool first in new[]{false,true})
        for(int mask=0;mask<4;mask++) {
            var p=new Program { RegenLocationType=(RegenLocationType)flags, FirstSpawn=first };
            var objects=Enumerable.Range(0,2).Select(i=>new WorldObject{Guid=new GuidStub{Full=(uint)i},Success=(mask&(1<<i))!=0}).ToList();
            var returned=p.Spawn(objects);
            Console.WriteLine($"{flags}|{(first?1:0)}|{mask}|{string.Join(',',returned.Select(o=>o.Guid.Full))}|{string.Join(',',objects.Where(o=>!o.Destroyed).Select(o=>o.Guid.Full))}|{string.Join(',',objects.Where(o=>o.Destroyed).Select(o=>o.Guid.Full))}");
        }
    }
}''')
    (path / 'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
    dotnet = '/tmp/bace-crafting-dotnet/dotnet'
    subprocess.run([dotnet,'build','--nologo','-o',str(path/'build')],cwd=path,check=True)
    output=subprocess.check_output([dotnet,str(path/'build/oracle.dll')],text=True)
target=Path(__file__).resolve().parents[1]/'tests/fixtures/generator_spawn.csv'
target.parent.mkdir(exist_ok=True)
target.write_text(''.join('# '+str(p.relative_to(SOURCE))+' '+hashlib.sha256(p.read_bytes()).hexdigest()+'\n' for p in [profile,enum])+'# flags|first|success_mask|returned|alive|destroyed\n'+output)
