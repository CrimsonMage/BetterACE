#!/usr/bin/env python3
"""Compile verbatim GDLE scalar primitives from the immutable local Git pin.
Only declarations and same-cell Position plumbing are supplied by the harness.
No collision predicate under test is reimplemented by the fixture generator.
"""
import argparse,hashlib,json,subprocess,tempfile
from pathlib import Path
PIN='353cbab52ef7da2b7063bc3e3f008461d8531693'
FILES=['Source/PhatSDK/'+x for x in ['Polygon.cpp','BSPData.cpp','MathLib.cpp','MathLib.h','Transition.cpp','PhatSDK.h']]
METHODS={
 'Polygon.cpp':['void CPolygon::make_plane()','BOOL CPolygon::polygon_hits_sphere_slow_but_sure(','int CPolygon::hits_sphere(','int CPolygon::pos_hits_sphere('],
 'BSPData.cpp':['BOOL CSphere::intersects(','int BSPNODE::sphere_intersects_solid(','int BSPNODE::sphere_intersects_poly(','int BSPLEAF::sphere_intersects_solid(','int BSPLEAF::sphere_intersects_poly('],
 'MathLib.cpp':['Vector cross_product(','float Plane::dot_product('],
 'Transition.cpp':['void CTransition::calc_num_steps('],
}
def extract(text,signature):
 start=text.index(signature);brace=text.index('{',start);level=1;end=brace+1
 while level:
  if text[end]=='{':level+=1
  elif text[end]=='}':level-=1
  end+=1
 return text[start:end]
def main():
 p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--cxx',default='c++');args=p.parse_args();root=Path(__file__).resolve().parent
 sources={}
 for path in FILES:
  pinned=subprocess.check_output(['git','-C',str(args.source),'show',PIN+':'+path])
  data=(args.source/path).read_bytes()
  if data!=pinned:raise ValueError('modified pinned source '+path)
  sources[path]=data
 math=sources['Source/PhatSDK/MathLib.h'].decode();scalar=math[math.index('#else',math.index('#if (defined(__AVX__))'))+len('#else'):math.index('#endif',math.index('#else',math.index('#if (defined(__AVX__))')))]
 eps=[l for l in sources['Source/PhatSDK/PhatSDK.h'].decode().splitlines() if l.startswith('#define F_EPSILON ')][0]
 methods='\n'.join(extract(sources['Source/PhatSDK/'+file].decode(),sig) for file,sigs in METHODS.items() for sig in sigs)
 harness=(root/'gdle_bsp_harness.cpp').read_text();program=harness.replace('/*SCALAR_VECTOR*/',scalar).replace('/*EPSILON*/',eps).replace('/*PINNED_METHODS*/',methods)
 with tempfile.TemporaryDirectory(prefix='gdle-bsp-oracle-') as tmp:
  tmp=Path(tmp);(tmp/'oracle.cpp').write_text(program)
  subprocess.run([args.cxx,'-std=c++17','-O0','-ffp-contract=off','-fno-fast-math',str(tmp/'oracle.cpp'),'-o',str(tmp/'oracle')],check=True)
  vectors=json.loads(subprocess.check_output([str(tmp/'oracle')],text=True))
 fixture={'repository':'https://gitlab.com/Scribble/gdlenhanced','commit':PIN,'source_sha256':{p:hashlib.sha256(b).hexdigest() for p,b in sources.items()},'harness_sha256':hashlib.sha256(harness.encode()).hexdigest(),'methods':METHODS,'precision':'GDLE scalar Vector branch, f32 math and original f64 temporaries; C++17 -O0 -ffp-contract=off -fno-fast-math. Not x87/SIMD equivalence.','scope':'Original polygon plane/contact, BSP overlap/contact ordering and non-viewer radius subdivision. Same-cell Position delta adapter only; no slide/step/cell-transfer solver.','vectors':vectors}
 (root.parent/'fixtures/gdle_bsp.json').write_text(json.dumps(fixture,indent=2)+'\n')
if __name__=='__main__':main()
