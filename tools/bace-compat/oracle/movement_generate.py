#!/usr/bin/env python3
"""Compile unchanged official ACE movement formulas and CMT parser. Synthetic only."""
import argparse, hashlib, json, subprocess, tempfile, urllib.request
from pathlib import Path
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
BASE='Source/ACE.DatLoader/'
FILES=[BASE+x+'.cs' for x in ['IUnpackable','BinaryReaderExtensions','UnpackableExtensions','DatFileType','DatFileTypeAttribute','DatDatabaseType','DatDatabaseTypeAttribute','DatFileTypeExtensionAttribute','DatFileTypeIdRangeAttribute','FileTypes/FileType','FileTypes/CombatManeuverTable','Entity/CombatManeuver','FileTypes/LandblockInfo','Entity/BuildInfo','Entity/CBldPortal','Entity/Frame','Entity/Stab','FileTypes/ContractTable','Entity/Contract','Entity/Position','Entity/LandDefs','Entity/AnimData']]+['Source/ACE.Entity/Enum/'+x+'.cs' for x in ['MotionStance','MotionCommand','AttackHeight','AttackType','Quadrant','HoldKey','PortalFlags','PhysicsState','AnimationHookDir']]+['Source/ACE.Server/Physics/Common/EncumbranceSystem.cs','Source/ACE.Server/Physics/Animation/MovementSystem.cs','Source/ACE.Server/Physics/Animation/MotionInterp.cs','Source/ACE.Server/Physics/Animation/Sequence.cs','Source/ACE.Server/Physics/Animation/AFrame.cs','Source/ACE.Server/Physics/Animation/AnimData.cs','Source/ACE.Server/Physics/Animation/AnimSequenceNode.cs','Source/ACE.Server/Physics/PhysicsGlobals.cs']
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);args=p.parse_args();root=Path(__file__).resolve().parent
sources={}
for name in FILES:
 data=(args.source/name).read_bytes()
 pinned=urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{name}',timeout=30).read()
 if data!=pinned:raise ValueError('source differs from official pinned commit: '+name)
 sources[name]=data
harness=(root/'movement_harness.cs').read_bytes()
with tempfile.TemporaryDirectory(prefix='bace-movement-oracle-') as temp:
 build=Path(temp)
 for i,(name,data) in enumerate(sources.items()):
  if not name.endswith(('/MotionInterp.cs','/Sequence.cs')):(build/(str(i)+'_'+Path(name).name)).write_bytes(data)
 text=sources['Source/ACE.Server/Physics/Animation/MotionInterp.cs'].decode()
 def method(name):
  start=text.index('        public void '+name+'(');brace=text.index('{',start);depth=1;i=brace+1
  while depth:
   depth+=(text[i]=='{')-(text[i]=='}');i+=1
  return text[start:i]
 constants='\n'.join(line for line in text.splitlines() if 'public const float ' in line)
 (build/'MotionMethods.cs').write_text('using System; using ACE.Entity.Enum; class MiniInterp { public StubWeenie WeenieObj=new(); public StubRaw RawState=new(); public float MyRunRate=1;'+constants+method('adjust_motion')+method('apply_run_to_command')+'} class StubWeenie {public bool IsCreature=true; public float Rate; public bool InqRunRate(ref float r){r=Rate;return true;}} class StubRaw {public HoldKey CurrentHoldKey=HoldKey.None;}')
 text=sources['Source/ACE.Server/Physics/Animation/Sequence.cs'].decode()
 root_methods=method('update_internal')+method('advance_to_next_animation')+method('apply_physics')
 (build/'RootMethods.cs').write_text('using System;using System.Collections.Generic;using System.Numerics;using ACE.Entity.Enum;using ACE.DatLoader.Entity;using ACE.Server.Physics;using ACE.Server.Physics.Animation;using ACE.Server.Physics.Hooks;class RootOracle {public LinkedList<AnimSequenceNode> AnimList=new();public LinkedListNode<AnimSequenceNode> FirstCyclic;public Vector3 Velocity,Omega;public PhysicsObj HookObj=null;void execute_hooks(AnimationFrame frame,AnimationHookDir direction){}'+root_methods+'}')
 (build/'Program.cs').write_bytes(harness)
 (build/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>')
 subprocess.run([args.dotnet,'build','--nologo','-o',str(build/'out'),str(build/'Oracle.csproj')],check=True)
 vectors=json.loads(subprocess.check_output([args.dotnet,str(build/'out/Oracle.dll')],text=True))
fixture={'repository':'https://github.com/ACEmulator/ACE','commit':PIN,'source_sha256':{p:hashlib.sha256(d).hexdigest() for p,d in sources.items()},'harness_sha256':hashlib.sha256(harness).hexdigest(),'harness':'Unmodified official C# scalar classes/CMT parser and verbatim MotionInterp methods with synthetic dependency adapters; Root harness initializes FirstCyclic to Last exactly as Sequence.append_animation does; earlier clips are one-time warmup. No runtime asset bytes.','vectors':vectors}
(root.parent/'fixtures/movement_rules.json').write_text(json.dumps(fixture,indent=2)+'\n')
