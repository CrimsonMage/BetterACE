#!/usr/bin/env python3
"""Execute unchanged pinned ACE static recall initializers and Position constructors.
Database stand-ins select missing class, absent Destination, or authored override.
No world/DAT/player data is copied into the fixture.
"""
from pathlib import Path
import argparse, hashlib, re, subprocess, tempfile
PIN = '47edade3bd3f6044b676d4eb877c4965c7eda62b'
parser = argparse.ArgumentParser()
parser.add_argument('--ace', type=Path, default=Path(__file__).resolve().parents[4] / '.reference' / ('ACE-' + PIN))
parser.add_argument('--dotnet', default='/tmp/bace-crafting-dotnet/dotnet')
a = parser.parse_args()
src = a.ace / 'Source'
location = src / 'ACE.Server/WorldObjects/Player_Location.cs'
position = src / 'ACE.Entity/Position.cs'
def method(text, signature):
    start = text.index(signature)
    end = text.index('{', start) + 1
    depth = 1
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    return text[start:end]
def field(text, name):
    start = text.index('        private static ', text.index(name) - 50)
    return text[start:text.index(';', start) + 1]
text = location.read_text()
fields = '\n'.join(field(text, name) for name in ['MarketplaceDrop', 'pkArenaLocs =', 'pklArenaLocs ='])
constructors = '\n'.join(method(position.read_text(), signature) for signature in [
    'public Position(Position pos)', 'public Position(uint blockCellID, float newPositionX'])
program = r'''
using System; using System.Collections.Generic; using System.Numerics; using System.Globalization;
class LandblockId { public uint Raw; public LandblockId(uint value) { Raw=value; } }
class Position { public LandblockId LandblockId; public Vector3 Pos; public Quaternion Rotation;
public float PositionX {set {Pos.X=value;}} public float PositionY {set {Pos.Y=value;}} public float PositionZ {set {Pos.Z=value;}}
public void SetPosition(Vector3 p) { throw new Exception("unexpected outdoor adjustment"); }
CONSTRUCTORS
}
enum PositionType { Destination }
class Weenie { public Position GetPosition(PositionType type) => Program.Mode==1 ? null : new Position(0xA9B40001,1.25f,-2.5f,3.75f,0,0,0.6f,0.8f); }
class World { public Weenie GetCachedWeenie(string name) => Program.Mode==0 ? null : new Weenie(); }
class DatabaseManager {public static World World=new World();}
class Player {
FIELDS
public static void Print() { var all=new List<Position>{MarketplaceDrop};all.AddRange(pkArenaLocs);all.AddRange(pklArenaLocs);for(int i=0;i<all.Count;i++){var p=all[i]; Console.WriteLine($"{Program.Mode},{i},{p.LandblockId.Raw},{Bits(p.Pos.X)},{Bits(p.Pos.Y)},{Bits(p.Pos.Z)},{Bits(p.Rotation.W)},{Bits(p.Rotation.X)},{Bits(p.Rotation.Y)},{Bits(p.Rotation.Z)}");}}
static uint Bits(float value) => BitConverter.SingleToUInt32Bits(value);
}
class Program {public static int Mode;static void Main(string[] args){Mode=int.Parse(args[0],CultureInfo.InvariantCulture);Player.Print();}}
'''.replace('CONSTRUCTORS', constructors).replace('FIELDS', fields)
with tempfile.TemporaryDirectory(prefix='bace-recall-locations-') as tmp:
    d=Path(tmp)
    (d/'Program.cs').write_text(program)
    (d/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><WarningLevel>0</WarningLevel></PropertyGroup></Project>')
    subprocess.run([a.dotnet,'build','--nologo','-o',str(d/'build')],cwd=d,check=True)
    output=''.join(subprocess.check_output([a.dotnet,str(d/'build/oracle.dll'),str(mode)],text=True) for mode in range(3))
    dest=Path(__file__).resolve().parents[1]/'tests/fixtures/recall_locations.csv'
    dest.write_text('# official ACE '+PIN+'\n'+''.join('# '+str(p.relative_to(src))+' sha256='+hashlib.sha256(p.read_bytes()).hexdigest()+'\n' for p in [location,position])+'# mode,index,cell,origin xyz bits,rotation wxyz bits\n'+output)
    print('original-source recall location vectors:',len(output.splitlines()))
