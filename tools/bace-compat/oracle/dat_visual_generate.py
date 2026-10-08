#!/usr/bin/env python3
"""Pinned, unmodified ACE visual asset decoders against synthetic records."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import tarfile
from concurrent.futures import ThreadPoolExecutor
import urllib.request
from dat_generate import PIN, BASE, FILES

EXTRA = ['FileTypes/' + n for n in ['GfxObj','SetupModel','Palette','PaletteSet','ClothingTable','Surface','SurfaceTexture','SecondaryAttributeTable']]
EXTRA += ['Entity/' + n for n in ['CVertexArray','SWVertex','Vec2Duv','Polygon','BSPTree','BSPNode','BSPLeaf','BSPPortal','PortalPoly','Sphere','Plane','LocationType','PlacementType','CylSphere','LightInfo','AnimationFrame','AnimationHook','AttackCone','ClothingBaseEffect','CloObjectEffect','CloTextureEffect','CloSubPalEffect','CloSubPalette','CloSubPaletteRange','Attribute2ndBase']]
HOOKS=['AttackHook','CallPESHook','CreateBlockingParticle','CreateParticleHook','DefaultScriptPartHook','DestroyParticleHook','DiffuseHook','DiffusePartHook','EtherealHook','LuminousHook','LuminousPartHook','NoDrawHook','ReplaceObjectHook','ScaleHook','SetLightHook','SetOmegaHook','SoundHook','SoundTableHook','SoundTweakedHook','StopParticleHook','TextureVelocityHook','TextureVelocityPartHook','TransparentHook','TransparentPartHook']
EXTRA += ['Entity/AnimationHooks/' + n for n in HOOKS]
ENUMS=['GfxObjFlags','SetupFlags','StipplingType','CullMode','BSPType','Placement','AnimationHookType','AnimationHookDir','CoverageMask','SurfaceType']
SOURCES = FILES + [BASE + n + '.cs' for n in EXTRA] + ['Source/ACE.Entity/Enum/' + n + '.cs' for n in ENUMS]

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--source',type=Path,required=True);parser.add_argument('--dotnet',required=True);parser.add_argument('--archive',type=Path);args=parser.parse_args()
    root=Path(__file__).resolve().parent
    def verified(path):
        data=(args.source/path).read_bytes()
        if args.archive:
            with tarfile.open(args.archive) as archive: official=archive.extractfile(f'ACE-{PIN}/{path}').read()
        else: official=urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{path}',timeout=30).read()
        if data!=official: raise ValueError('Changed upstream source '+path)
        return path,data
    with ThreadPoolExecutor(max_workers=4) as pool: sources=dict(pool.map(verified,SOURCES))
    with tempfile.TemporaryDirectory(prefix='ace-visual-oracle-') as d:
        build=Path(d)
        for i,(path,data) in enumerate(sources.items()): (build/(str(i)+'_'+Path(path).name)).write_bytes(data)
        (build/'Program.cs').write_bytes((root/'dat_visual_harness.cs').read_bytes())
        (build/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>')
        subprocess.run([args.dotnet,'build','--nologo','-o',str(build/'out'),str(build/'Oracle.csproj')],check=True)
        output=subprocess.check_output([args.dotnet,str(build/'out/Oracle.dll')],text=True)
    result={'repository':'https://github.com/ACEmulator/ACE','commit':PIN,'source_sha256':{p:hashlib.sha256(d).hexdigest() for p,d in sources.items()},'harness_sha256':hashlib.sha256((root/'dat_visual_harness.cs').read_bytes()).hexdigest(),'vectors':json.loads(output)}
    (root.parent/'fixtures/dat_visual.json').write_text(json.dumps(result,indent=2)+'\n')
if __name__=='__main__': main()
