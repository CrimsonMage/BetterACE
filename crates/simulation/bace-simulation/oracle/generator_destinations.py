#!/usr/bin/env python3
"""Compile unchanged ACE destination dispatch; inventory acceptance is controlled.
ACEmulator contributors, AGPL-3.0-only. This proves subtype dispatch, not complete
Container or Vendor inventory behavior (covered by their separate fixtures).
"""
from pathlib import Path
import hashlib
import subprocess
import tempfile
ROOT = Path(__file__).resolve().parents[4]
source = ROOT / '.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Server/Entity/GeneratorProfile.cs'
text = source.read_text()
def method(name):
    start = text.index('public bool ' + name)
    brace = text.index('{', start)
    depth, end = 1, brace + 1
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    return text[start:end]
with tempfile.TemporaryDirectory(prefix='bace-generator-destination-') as directory:
    p = Path(directory)
    (p / 'Program.cs').write_text('''
using System;
class WorldObject { public uint Guid=1, WeenieClassId=10; public string Name="object"; }
class Container: WorldObject { public bool Accept; public WorldObject Held; public bool TryAddToInventory(WorldObject o) { if(Accept)Held=o;return Accept; } }
class Creature: Container {}
class Vendor: Creature { public void AddDefaultItem(WorldObject o) { Held=o; } }
class Log { public void Warn(string s) {} }
class Profile { public WorldObject Generator; Log log=new();
''' + method('Spawn_Container') + method('Spawn_Shop') + '''}
class Program { static void Main() {
 for(int owner=0;owner<3;owner++) for(int subtype=0;subtype<2;subtype++) foreach(bool accept in new[]{false,true}) {
 WorldObject generator=owner==0?new WorldObject():owner==1?new Container{Accept=accept}:new Vendor{Accept=accept};
 WorldObject child=subtype==0?new WorldObject():new Creature();
 var p=new Profile{Generator=generator};
 bool contain=p.Spawn_Container(child), shop=p.Spawn_Shop(child);
 Console.WriteLine($"{owner}|{subtype}|{(accept?1:0)}|{(contain?1:0)}|{(shop?1:0)}");
 } }}''')
    (p / 'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
    dotnet = '/tmp/bace-crafting-dotnet/dotnet'
    subprocess.run([dotnet, 'build', '--nologo', '-o', str(p / 'build')], cwd=p, check=True)
    output = subprocess.check_output([dotnet, str(p / 'build/oracle.dll')], text=True)
(Path(__file__).resolve().parents[1] / 'tests/fixtures/generator_destinations.csv').write_text('# ACE.Server/Entity/GeneratorProfile.cs ' + hashlib.sha256(source.read_bytes()).hexdigest() + '\n# owner(object/container/vendor)|child(object/creature)|accept|contain|shop\n' + output)
