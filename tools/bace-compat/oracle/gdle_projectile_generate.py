#!/usr/bin/env python3
"""Pinned GDLE airborne physics scalar oracle; synthetic inputs only."""
import argparse, hashlib, json, subprocess, tempfile
from pathlib import Path
PIN='353cbab52ef7da2b7063bc3e3f008461d8531693'
FILES=['Source/PhatSDK/'+p for p in ['PhysicsObj.cpp','MathLib.h','PhatSDK.h','LandDefs.cpp','Frame.cpp']]
def method(text,signature):
 start=text.index(signature);brace=text.index('{',start);depth=1;end=brace+1
 while depth:depth+=(text[end]=='{')-(text[end]=='}');end+=1
 return text[start:end]
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--cxx',default='c++');args=p.parse_args();root=Path(__file__).resolve().parent
sources={}
for path in FILES:
 raw=(args.source/path).read_bytes();pinned=subprocess.check_output(['git','-C',str(args.source),'show',PIN+':'+path])
 if raw!=pinned:raise ValueError('modified source '+path)
 sources[path]=raw
math=sources[FILES[1]].decode();start=math.index('#else',math.index('#if (defined(__AVX__))'))+len('#else');scalar=math[start:math.index('#endif',start)]
scalar+='\n'+method(math,'\tBOOL is_equal(')+'\n'+method(math,'\tbool operator==(const Vector& v)')+'\n'+method(math,'\tbool operator!=(const Vector& v)')
physics=sources[FILES[0]].decode();code=method(physics,'void CPhysicsObj::UpdatePhysicsInternal(')
setter=physics[physics.index('void CPhysicsObj::set_velocity('):];prefix=method(setter,'\tif (m_velocityVector != new_velocity)')
land=sources[FILES[3]].decode();declarations=land[land.index('\tuint32_t blockid_mask'):land.index('\tclass FillHeightTable')]
code+='\nnamespace LandDefs {'+declarations+method(land,'\tBOOL blockid_to_lcoord(')+method(land,'\tVector get_block_offset(')+'}\n'+method(sources[FILES[4]].decode(),'Vector Position::get_offset(const Position& pos) const')
code+='\nvoid CPhysicsObj::set_test_velocity(const Vector &new_velocity){'+prefix+'}'
constants='\n'.join(line for line in physics.splitlines() if line.startswith(('const float small_velocity =','const float max_velocity =')))
epsilon=next(line for line in sources[FILES[2]].decode().splitlines() if line.startswith('#define F_EPSILON '))
harness=(root/'gdle_projectile_harness.cpp').read_bytes();program=harness.decode().replace('/*VECTOR*/',scalar).replace('/*CONSTANTS*/',constants+'\n'+epsilon).replace('/*METHODS*/',code)
with tempfile.TemporaryDirectory(prefix='gdle-projectile-') as directory:
 directory=Path(directory);(directory/'oracle.cpp').write_text(program)
 subprocess.run([args.cxx,'-std=c++17','-O0','-fno-fast-math','-ffp-contract=off',str(directory/'oracle.cpp'),'-o',str(directory/'oracle')],check=True)
 vectors=json.loads(subprocess.check_output([str(directory/'oracle')],text=True))
fixture={'repository':'https://gitlab.com/Scribble/gdlenhanced','commit':PIN,'source_sha256':{p:hashlib.sha256(v).hexdigest() for p,v in sources.items()},'harness_sha256':hashlib.sha256(harness).hexdigest(),'scope':'Original scalar Vector branch and entire UpdatePhysicsInternal, plus original set_velocity first conditional. Airborne only: friction adapter aborts for ON_WALKABLE, orientation adapter aborts for nonzero omega. No collision/lifetime/wall-clock/network claims. Proprietary retail decompile is not embedded.','precision':'C++17 -O0 -fno-fast-math -ffp-contract=off, f32 fields and admitted quantum; no x87/SIMD bit equivalence claim.','vectors':vectors}
(root.parent/'fixtures/gdle_projectile.json').write_text(json.dumps(fixture,indent=2)+'\n')
