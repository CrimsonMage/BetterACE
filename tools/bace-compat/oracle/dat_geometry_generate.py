#!/usr/bin/env python3
"""Independent unmodified pinned C# EnvCell/BSP oracle; synthetic data only."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib,json,subprocess,tempfile,urllib.request
from pathlib import Path
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
BASE='Source/ACE.DatLoader/'
FILES=[BASE+x+'.cs' for x in ['IUnpackable','BinaryReaderExtensions','UnpackableExtensions','DatFileType','DatFileTypeAttribute','DatDatabaseType','DatDatabaseTypeAttribute','DatFileTypeExtensionAttribute','DatFileTypeIdRangeAttribute','FileTypes/FileType','FileTypes/EnvCell','Entity/Frame','Entity/CellPortal','Entity/Stab','Entity/Plane','Entity/Sphere','Entity/PortalPoly','Entity/BSPTree','Entity/BSPNode','Entity/BSPLeaf','Entity/BSPPortal']]+['Source/ACE.Entity/Enum/'+x+'.cs' for x in ['EnvCellFlags','PortalFlags','BSPType']]
FILES += [BASE+x+".cs" for x in ['Entity/AnimData', 'Entity/AnimationFrame', 'Entity/AnimationHook', 'Entity/AnimationHooks/AttackHook', 'Entity/AnimationHooks/CallPESHook', 'Entity/AnimationHooks/CreateBlockingParticle', 'Entity/AnimationHooks/CreateParticleHook', 'Entity/AnimationHooks/DefaultScriptPartHook', 'Entity/AnimationHooks/DestroyParticleHook', 'Entity/AnimationHooks/DiffuseHook', 'Entity/AnimationHooks/DiffusePartHook', 'Entity/AnimationHooks/EtherealHook', 'Entity/AnimationHooks/LuminousHook', 'Entity/AnimationHooks/LuminousPartHook', 'Entity/AnimationHooks/NoDrawHook', 'Entity/AnimationHooks/ReplaceObjectHook', 'Entity/AnimationHooks/ScaleHook', 'Entity/AnimationHooks/SetLightHook', 'Entity/AnimationHooks/SetOmegaHook', 'Entity/AnimationHooks/SoundHook', 'Entity/AnimationHooks/SoundTableHook', 'Entity/AnimationHooks/SoundTweakedHook', 'Entity/AnimationHooks/StopParticleHook', 'Entity/AnimationHooks/TextureVelocityHook', 'Entity/AnimationHooks/TextureVelocityPartHook', 'Entity/AnimationHooks/TransparentHook', 'Entity/AnimationHooks/TransparentPartHook', 'Entity/AnimationPartChange', 'Entity/AttackCone', 'Entity/MotionData', 'FileTypes/Animation', 'FileTypes/MotionTable']]
FILES += ["Source/ACE.Entity/Enum/"+x+".cs" for x in ["MotionDataFlags","MotionStance","MotionCommand","AnimationFlags","AnimationHookType","AnimationHookDir"]]+["Source/ACE.Entity/AttackFrameParams.cs"]
FILES += [BASE+x+".cs" for x in ["FileTypes/Environment","Entity/CellStruct","Entity/CVertexArray","Entity/SWVertex","Entity/Polygon","Entity/Vec2Duv"]]+["Source/ACE.Entity/Enum/"+x+".cs" for x in ["CullMode","StipplingType"]]
FILES += [BASE+x+".cs" for x in ["FileTypes/SetupModel","Entity/CylSphere","Entity/LocationType","Entity/PlacementType","Entity/LightInfo"]]+["Source/ACE.Entity/Enum/"+x+".cs" for x in ["SetupFlags","Placement"]]
def main():
 p=argparse.ArgumentParser();p.add_argument('--source',type=Path);p.add_argument('--dotnet',default='dotnet');args=p.parse_args();root=Path(__file__).resolve().parent
 def verified(path):
  official=urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{path}',timeout=30).read()
  data=(args.source/path).read_bytes() if args.source else official
  if data!=official:raise ValueError('source differs from immutable official pin: '+path)
  return path,data
 with ThreadPoolExecutor(max_workers=4) as pool:sources=dict(pool.map(verified,FILES))
 with tempfile.TemporaryDirectory(prefix='ace-geometry-oracle-') as temporary:
  build=Path(temporary)
  for i,(path,data) in enumerate(sources.items()):(build/(str(i)+'_'+Path(path).name)).write_bytes(data)
  harness=(root/'dat_geometry_harness.cs').read_bytes();(build/'Program.cs').write_bytes(harness)
  (build/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>')
  subprocess.run([args.dotnet,'build','--nologo','-o',str(build/'out'),str(build/'Oracle.csproj')],check=True)
  output=subprocess.check_output([args.dotnet,str(build/'out/Oracle.dll')],text=True)
 fixture={'repository':'https://github.com/ACEmulator/ACE','commit':PIN,'source_sha256':{p:hashlib.sha256(d).hexdigest() for p,d in sources.items()},'harness_sha256':hashlib.sha256(harness).hexdigest(),'harness':'Synthetic BinaryWriter geometry; unmodified official parsers. Unused Position/DatManager/log4net dependencies stubbed; no parser or calculation is stubbed.','vectors':json.loads(output)}
 (root.parent/'fixtures/dat_geometry.json').write_text(json.dumps(fixture,indent=2)+'\n')
if __name__=='__main__':main()
