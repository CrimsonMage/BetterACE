#!/usr/bin/env python3
"""Compile unchanged ACE origin, strike velocity and launch placement statements."""
import argparse, hashlib, subprocess, tempfile, urllib.request
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args()
pin='47edade3bd3f6044b676d4eb877c4965c7eda62b'
sources={}
for rel in ['Source/ACE.Server/WorldObjects/WorldObject_Magic.cs','Source/ACE.Server/Physics/Trajectory.cs']:
 data=(a.source/rel).read_bytes();assert data==urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{pin}/{rel}',timeout=30).read();sources[rel]=data
world,trajectory=[v.decode('utf-8-sig') for v in sources.values()]
def method(s,name):
 start=s.index(name);opening=s.index('{',start);depth=0
 for end in range(opening,len(s)):
  depth+=(s[end]=='{')-(s[end]=='}')
  if depth==0:return s[start:end+1]
 raise ValueError(name)
wm='\n'.join(method(world,n) for n in ['public Vector3 CalculatePreOffset(', 'public List<Vector3> CalculateProjectileOrigins(', 'public static float GetSpreadAnglePerStep(', 'public Vector3 CalculateProjectileVelocity('])
tm='\n'.join(method(trajectory,n) for n in ['public static bool IsZero(', 'public static int SolveQuadric(', 'public static bool solve_ballistic_arc_lateral(Vector3 proj_pos, float lateral_speed, Vector3 target, Vector3 target_velocity'])
launch=world[world.index('public List<SpellProjectile> LaunchSpellProjectiles('):]
placement=launch[launch.index('var rotate = casterLoc.Rotation;'):launch.index('// set orientation')]
program=r'''using System;using System.Numerics;using System.Collections.Generic;
static class Extensions {public static float ToRadians(this float f)=>f*(float)Math.PI/180f;}
enum ProjectileSpellType {Bolt,Arc,Ring,Strike}
class SpellData {public int? DimsOriginX,DimsOriginY,DimsOriginZ;}
class Spell {public int NumProjectiles;public float SpreadAngle;public Vector3 CreateOffset,Peturbation,Padding;public bool IsTracking;public SpellData _spell=new();}
class Position {public Vector3 Pos;public Quaternion Rotation=Quaternion.Identity;public int Landblock=1;public Position(){}public Position(Position p){Pos=p.Pos;Rotation=p.Rotation;}public Vector3 ToGlobal(bool b)=>Pos;}
class PhysicsPosition {public Position Loc=new();public Position ACEPosition()=>new(Loc);public Vector3 GetOffset(PhysicsPosition p)=>p.Loc.Pos-Loc.Pos;}
class Phys {public PhysicsPosition Position=new();public Vector3 CachedVelocity,Velocity;public float GetPhysicsRadius()=>.5f;}
namespace Physics.Common {class Position {public Frame Frame;public Position(global::PhysicsPosition p){Frame=new Frame{Origin=p.Loc.Pos};}public Vector3 GetOffset(Position p)=>p.Frame.Origin-Frame.Origin;}class Frame {public Vector3 Origin;}}
static class ThreadSafeRandom {public static double Next(float a,float b)=>.25;}
static class PropertyManager {public static (bool Item,int other) GetBool(string s)=>(false,0);}
static class PhysicsGlobals {public const float Gravity=-9.8f;}
static class Trajectory2 {public static Vector3 CalculateTrajectory(Vector3 a,Vector3 b,Vector3 c,float s,bool g)=>throw new Exception("alternate solver not qualified");}
static class Trajectory {// TM
}
class Creature:WorldObject {public WorldObject AttackTarget;}
class Player:Creature{}
class Shot {public Position Location;public Phys PhysicsObj=new();}
class WorldObject {public float Height=1.9f;public Phys PhysicsObj=new();public const float ProjHeight=2f/3f,ProjHeightArc=5f/6f;public static readonly Quaternion OneEighty=Quaternion.CreateFromAxisAngle(Vector3.UnitZ,(float)Math.PI);public float GetProjectileRadius(Spell s)=>.15f;public float GetProjectileSpeed(Spell s)=>20;public float GetCylinderDistance(WorldObject t)=>(t.PhysicsObj.Position.Loc.Pos-PhysicsObj.Position.Loc.Pos).Length()-1f;
// WM
public List<Shot> Launch(Spell spell, WorldObject target,List<Vector3> origins,Vector3 velocity) {bool strikeSpell=true;var casterLoc=PhysicsObj.Position.ACEPosition();var targetLoc=target.PhysicsObj.Position.ACEPosition();var shots=new List<Shot>();foreach(var origin in origins){var sp=new Shot();
// PLACEMENT
shots.Add(sp);}return shots;}}
class Program {static void Main(){foreach(int count in new[]{1,3})foreach(float heading in new[]{0f,.7f})foreach(bool tracking in new[]{false,true})foreach(bool perturb in new[]{false,true})foreach(int elevated in new[]{0,1}){
 var source=new WorldObject();source.PhysicsObj.Position.Loc.Pos=new Vector3(1,2,0);source.PhysicsObj.Position.Loc.Rotation=Quaternion.CreateFromAxisAngle(Vector3.UnitZ,heading);
 var target=new WorldObject{Height=1.8f};target.PhysicsObj.Position.Loc.Pos=new Vector3(10,30,elevated*3);target.PhysicsObj.CachedVelocity=new Vector3(3,-1,.5f);
 var spell=new Spell{NumProjectiles=count,SpreadAngle=0,CreateOffset=new Vector3(.1f,.2f,-.1f),Peturbation=perturb?new Vector3(.2f,.3f,.4f):Vector3.Zero,Padding=new Vector3(.4f,.5f,.6f),IsTracking=tracking};spell._spell.DimsOriginX=count;spell._spell.DimsOriginY=1;spell._spell.DimsOriginZ=1;
 var origins=source.CalculateProjectileOrigins(spell,ProjectileSpellType.Strike,target);var velocity=source.CalculateProjectileVelocity(spell,target,ProjectileSpellType.Strike,origins[0]);var shots=source.Launch(spell,target,origins,velocity);
 for(int i=0;i<shots.Count;i++){var pos=shots[i].Location.Pos;var v=shots[i].PhysicsObj.Velocity;Console.WriteLine($"{count},{heading:R},{(tracking?1:0)},{(perturb?1:0)},{elevated},{i},{pos.X:R},{pos.Y:R},{pos.Z:R},{v.X:R},{v.Y:R},{v.Z:R}");}
}}}
'''.replace('// WM',wm).replace('// TM',tm).replace('// PLACEMENT',placement)
with tempfile.TemporaryDirectory(prefix='ace-strike-') as td:
 b=Path(td);(b/'Program.cs').write_text(program);(b/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 subprocess.run([a.dotnet,'build',str(b/'oracle.csproj'),'-o',str(b/'out'),'--nologo','-v:q'],check=True);output=subprocess.check_output([a.dotnet,str(b/'out/oracle.dll')],text=True)
headers=[f'# ACE {pin}; AGPL-3.0-only, ACE contributors; trajectory roots originally Jochen Schwarze, Graphics Gems I']+[f'# sha256 {hashlib.sha256(v).hexdigest()} {k}' for k,v in sources.items()]+['# Original origins/strike velocity/default lateral solver and launch placement; synthetic accepted poses and explicit RNG; no alternate trajectory solver.']
(Path(__file__).parent.parent/'tests/fixtures/strike.csv').write_text('\n'.join(headers)+'\n'+output)
