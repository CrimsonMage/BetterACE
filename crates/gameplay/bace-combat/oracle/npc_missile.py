#!/usr/bin/env python3
"""Execute unchanged official ACE methods using synthetic read-only quality adapters."""
import argparse, hashlib, subprocess, tempfile, urllib.request
from pathlib import Path
PIN = '47edade3bd3f6044b676d4eb877c4965c7eda62b'
p = argparse.ArgumentParser()
p.add_argument('--source', type=Path, required=True)
p.add_argument('--dotnet', required=True)
a = p.parse_args()
def method(source, signature):
    start = source.index(signature)
    opening = source.index('{', start)
    depth = 0
    for i in range(opening, len(source)):
        depth += (source[i] == '{') - (source[i] == '}')
        if depth == 0:
            return source[start:i+1]
    raise ValueError(signature)
methods = []
headers = [f'# Official ACE {PIN}; unchanged NPC missile methods; AGPL-3.0-only']
for filename, signature in [('Creature_Missile.cs', 'public float GetMaxMissileRange()'), ('Creature_Combat.cs', 'public float GetAnimSpeed()')]:
    relative = 'Source/ACE.Server/WorldObjects/' + filename
    data = (a.source / relative).read_bytes()
    assert data == urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{relative}', timeout=30).read()
    headers.append(f'# sha256 {hashlib.sha256(data).hexdigest()} {relative}')
    methods.append(method(data.decode('utf-8-sig'), signature))
source = '''using System;
class Weapon { public double MaximumVelocity; }
class Skill { public uint Current; }
class Creature {
 public const float MissileRangeCap = 85.0f / 1.094f, DefaultMaxVelocity = 20.0f;
 public const double MinAttackSpeed = 0.5, MaxAttackSpeed = 2.0;
 public Weapon Weapon = new Weapon(); public Skill Quickness = new Skill(); public uint WeaponSpeed;
 Weapon GetEquippedMissileWeapon() => Weapon;
 static uint GetWeaponSpeed(Creature creature) => creature.WeaponSpeed;
 METHODS
 static void Main() {
  foreach(double velocity in new double[]{0,1,10,20,27.5,28,100,1000}) {
   var c = new Creature(); c.Weapon.MaximumVelocity = velocity;
   Console.WriteLine($"range,{velocity:R},0,{c.GetMaxMissileRange():R}");
  }
  foreach(uint quickness in new uint[]{0,10,100,300,1000}) foreach(uint speed in new uint[]{0,40,100,500}) {
   var c = new Creature(); c.Quickness.Current = quickness; c.WeaponSpeed = speed;
   Console.WriteLine($"speed,{quickness},{speed},{c.GetAnimSpeed():R}");
  }
 }
}'''.replace('METHODS', '\n'.join(methods))
with tempfile.TemporaryDirectory(prefix='bace-npc-missile-') as td:
    directory = Path(td)
    (directory / 'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
    (directory / 'Program.cs').write_text(source)
    subprocess.run([a.dotnet, 'build', '--nologo', '-o', str(directory / 'out'), str(directory / 'Oracle.csproj')], check=True)
    result = subprocess.check_output([a.dotnet, str(directory / 'out/Oracle.dll')], text=True)
(Path(__file__).parent.parent / 'tests/fixtures/npc_missile.csv').write_text('\n'.join(headers) + '\n' + result)
