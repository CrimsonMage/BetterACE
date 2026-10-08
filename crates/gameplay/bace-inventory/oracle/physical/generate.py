#!/usr/bin/env python3
"""Compile unchanged pinned ACE pickup selection and cylinder-distance methods."""
from pathlib import Path
import subprocess, tempfile, os, hashlib
ROOT=Path(__file__).resolve().parents[5]
ACE=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Server'
a=(ACE/'WorldObjects/Player_Inventory.cs').read_text()
b=(ACE/'Physics/Common/Position.cs').read_text()
def method(source,name):
    start=source.index(name);start=source.rfind('\n',0,start)+1
    begin=source.index('{',start); depth=1; end=begin+1
    while depth:
        if source[end]=='{':depth+=1
        elif source[end]=='}':depth-=1
        end+=1
    return source[start:end]
pick=method(a,'private MotionCommand GetPickupMotion(')
dist=method(b,'public static double CylinderDistance(')
code='''using System; using System.Numerics; using System.Globalization;
enum MotionCommand:uint {Invalid=0,Pickup=0x40000018,Pickup5=0x40000136,Pickup10=0x40000137,Pickup15=0x40000138,Pickup20=0x40000139}
class Frame {public Vector3 Origin;}
class Position {public Frame Frame=new(); public float PositionZ=>Frame.Origin.Z; public Vector3 GetOffset(Position b)=>b.Frame.Origin-Frame.Origin;
DIST
}
class WorldObject {public Position Location=new();public float Height;}
class Corpse:WorldObject{}
static class PropertyManager {public static (bool Item,bool _) GetBool(string name)=>(false,false);}
class Player:WorldObject {public bool IsJumping=false;
PICK
public uint Select(WorldObject target)=>(uint)GetPickupMotion(target);
}
class Program {static void Main(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
foreach(float pz in new[]{-5f,0f,100f})foreach(float ph in new[]{0.5f,2f})foreach(float z in new[]{-1f,0f,0.199999f,0.2f,0.499999f,0.5f,0.7f,0.9f,1.1f})foreach(bool corpse in new[]{false,true}){
var p=new Player{Height=ph};p.Location.Frame.Origin=new Vector3(0,0,pz);
WorldObject t=corpse?new Corpse():new WorldObject();t.Height=0.4f;t.Location.Frame.Origin=new Vector3(0,0,pz+ph*z);
Console.WriteLine($"motion {pz:R} {ph:R} {t.Location.PositionZ:R} {t.Height:R} {(corpse?1:0)} {p.Select(t)}");}
foreach(float x in new[]{0f,0.1f,1f,10f})foreach(float z in new[]{-3f,0f,0.5f,3f})foreach(float r in new[]{0.1f,1f}){
var a=new Position();var b=new Position();b.Frame.Origin=new Vector3(x,0.3f,z);
Console.WriteLine($"distance 0 0 0 {r:R} 2 {x:R} 0.3 {z:R} 0.25 0.5 {Position.CylinderDistance(r,2,a,0.25f,0.5f,b):R}");}
}}
'''.replace('PICK',pick).replace('DIST',dist)
with tempfile.TemporaryDirectory(prefix='betterace-inventory-physical-') as d:
    p=Path(d);(p/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
    (p/'Program.cs').write_text(code)
    sdk=os.environ.get('DOTNET','/tmp/bace-crafting-dotnet/dotnet')
    out=subprocess.check_output([sdk,'run','--project',str(p/'Oracle.csproj'),'-c','Release'],text=True)
    rows='\n'.join(line for line in out.splitlines() if line.startswith(('motion ','distance ')))+'\n'
    dest=ROOT/'crates/gameplay/bace-inventory/tests/fixtures/physical.txt'
    dest.write_text('# ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b\n# Player_Inventory.cs sha256 '+hashlib.sha256(a.encode()).hexdigest()+'\n# Position.cs sha256 '+hashlib.sha256(b.encode()).hexdigest()+'\n'+rows)
    print(len(rows.splitlines()),'original ACE vectors')
