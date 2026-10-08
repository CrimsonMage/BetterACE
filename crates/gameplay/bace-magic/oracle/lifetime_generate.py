#!/usr/bin/env python3
"""Original GDLE Tick, Env/Obj collisions; accepted-target delay statement only."""
import argparse,hashlib,subprocess,tempfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);a=p.parse_args();pin='353cbab52ef7da2b7063bc3e3f008461d8531693';rel='Source/SpellProjectile.cpp';data=(a.source/rel).read_bytes();assert data==subprocess.check_output(['git','-C',str(a.source),'show',f'{pin}:{rel}']);s=data.decode('utf-8-sig')
def method(signature):
 start=s.index(signature);opening=s.index('{',start);depth=0
 for i in range(opening,len(s)):
  depth+=(s[i]=='{')-(s[i]=='}')
  if depth==0:return s[start:i+1]
methods='\n'.join(method(sig)for sig in ['void CSpellProjectile::Tick()','BOOL CSpellProjectile::DoCollision(const class EnvCollisionProfile','BOOL CSpellProjectile::DoCollision(const class ObjCollisionProfile'])
harness=r'''
#include <iostream>
#include <iomanip>
#include <cfloat>
using BOOL=int;namespace Timer{double cur_time=0;}constexpr float MAX_SPELL_PROJECTILE_LIFETIME=30.0f;
class EnvCollisionProfile{};class ObjCollisionProfile{};
class CWeenieObject{public:int DoCollision(const EnvCollisionProfile&){return 0;}int DoCollision(const ObjCollisionProfile&){return 0;}};
struct Position {float value=0;float distance(Position){return value;}};
class CSpellProjectile:public CWeenieObject {public:bool m_bDestroyMe=false,valid=true;double m_fSpawnTime=0,m_fDestroyTime=FLT_MAX;Position m_Position;struct {Position initial_cast_position;float max_range=75;}m_CachedSpellCastData;int explosions=0;bool InValidCell(){return valid;}void MarkForDestroy(){m_bDestroyMe=true;}void HandleExplode(){explosions++;}void Movement_UpdatePos(){}void Tick();BOOL DoCollision(const EnvCollisionProfile&);BOOL DoCollision(const ObjCollisionProfile&);};
// METHODS
int main(){std::cout<<std::setprecision(12);for(int mode:{0,1,2,3,4})for(float distance:{0.0f,75.0f,75.0001f}){CSpellProjectile p;p.m_Position.value=distance;Timer::cur_time=0;if(mode==1)p.DoCollision(EnvCollisionProfile{});if(mode==2){p.HandleExplode(); // TARGET_DELAY
}if(mode==3)p.valid=false;if(mode==4)p.DoCollision(ObjCollisionProfile{});
for(double t:{0.0,.49,.5,.999,1.0,28.999,29.0,29.5,30.0,31.0}){Timer::cur_time=t;p.Tick();std::cout<<mode<<","<<distance<<","<<t<<","<<p.explosions<<","<<p.m_bDestroyMe<<"\n";}}}
'''.replace('// METHODS',methods).replace('// TARGET_DELAY',s[s.index('m_fDestroyTime = Timer::cur_time + 0.5;'):s.index('m_fDestroyTime = Timer::cur_time + 0.5;')+len('m_fDestroyTime = Timer::cur_time + 0.5;')].replace('m_fDestroyTime','p.m_fDestroyTime'))
with tempfile.TemporaryDirectory(prefix='gdle-lifetime-')as td:
 b=Path(td);(b/'main.cpp').write_text(harness);subprocess.run(['c++','-std=c++17','-O0',str(b/'main.cpp'),'-o',str(b/'main')],check=True);result=subprocess.check_output([str(b/'main')],text=True)
(Path(__file__).parent.parent/'tests/fixtures/projectile_lifetime.csv').write_text(f'# GDLE {pin}; AGPL-3.0-only GDLE contributors\n# sha256 {hashlib.sha256(data).hexdigest()} {rel}\n# Original Tick/Env/Obj collision; target delay statement only, damage policy not emulated.\n'+result)
