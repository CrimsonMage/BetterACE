#!/usr/bin/env python3
"""Execute pinned GDLE launch velocity method with owned vector/frame adapters."""
import argparse,subprocess,tempfile,hashlib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);a=p.parse_args();pin='353cbab52ef7da2b7063bc3e3f008461d8531693';rel='Source/SpellcastingManager.cpp';data=(a.source/rel).read_bytes()
assert data==subprocess.check_output(['git','-C',str(a.source),'show',f'{pin}:{rel}'])
s=data.decode('utf-8-sig');start=s.index('Vector CSpellcastingManager::GetSpellProjectileSpawnVelocity(');opening=s.index('{',start);depth=0
for end in range(opening,len(s)):
 depth+=(s[end]=='{')-(s[end]=='}')
 if depth==0:break
method=s[start:end+1]
harness='''#include <cmath>
#include <cfloat>
#include <algorithm>
#include <iomanip>
#include <iostream>
using std::min;
struct Vector{float x,y,z;Vector(float a=0,float b=0,float c=0):x(a),y(b),z(c){}float magnitude(){return sqrtf(x*x+y*y+z*z);}Vector operator/(float n){return Vector(x/n,y/n,z/n);}bool normalize_check_small(){float n=magnitude();if(n<0.0002f)return true;float inv=1/n;x*=inv;y*=inv;z*=inv;return false;}void normalize(){normalize_check_small();}};
struct Position {Vector p;Position add_offset(Vector v){return Position{Vector(p.x+v.x,p.y+v.y,p.z+v.z)};}Vector get_offset(Position other){return Vector(other.p.x-p.x,other.p.y-p.y,other.p.z-p.z);}Vector localtoglobalvec(Vector v){return v;}};
struct CWeenieObject {Position m_Position;Vector velocity;float GetHeight(){return 0;}Vector get_velocity(){return velocity;}};
struct CSpellcastingManager{CWeenieObject source;CWeenieObject*GetCastSource(){return &source;}Vector GetSpellProjectileSpawnVelocity(Position*,CWeenieObject*,float,bool,bool,Vector*,double,bool);};
// METHOD
int main(){std::cout<<std::setprecision(9);for(auto offset:{Vector(0,20,0),Vector(10,30,2)})for(auto velocity:{Vector(0,0,0),Vector(3,-1,0.5),Vector(0.0001f,0,0),Vector(0.0002f,0,0)})for(float speed:{20.0f,50.0f})for(bool tracked:{false,true})for(bool gravity:{false,true}){
CSpellcastingManager manager;Position origin;CWeenieObject target{{offset},velocity};auto v=manager.GetSpellProjectileSpawnVelocity(&origin,&target,speed,tracked,gravity,nullptr,0,false);
std::cout<<offset.x<<","<<offset.y<<","<<offset.z<<","<<velocity.x<<","<<velocity.y<<","<<velocity.z<<","<<speed<<","<<tracked<<","<<gravity<<","<<v.x<<","<<v.y<<","<<v.z<<"\\n";}}
'''.replace('// METHOD',method)
with tempfile.TemporaryDirectory(prefix='gdle-trajectory-') as td:
 d=Path(td);(d/'main.cpp').write_text(harness);subprocess.run(['c++','-std=c++17','-O0','-fno-fast-math','-ffp-contract=off',str(d/'main.cpp'),'-o',str(d/'main')],check=True);output=subprocess.check_output([str(d/'main')],text=True)
(Path(__file__).parent.parent/'tests/fixtures/trajectory.csv').write_text(f'# GDLE {pin}; AGPL-3.0-only GDLE contributors\n# sha256 {hashlib.sha256(data).hexdigest()} {rel}\n# Original launch-time velocity method; synthetic offset adapters, GDLE scalar float division/0.0002 normalization adapter, angle zero.\n'+output)
