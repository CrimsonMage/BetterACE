#!/usr/bin/env python3
"""Compile pinned GDLE spawn/velocity and original scalar frame/math methods."""
import argparse,hashlib,subprocess,tempfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);a=p.parse_args()
pin='353cbab52ef7da2b7063bc3e3f008461d8531693';sources={}
for rel in ['Source/SpellcastingManager.cpp','Source/PhatSDK/Frame.cpp','Source/PhatSDK/MathLib.cpp','Source/PhatSDK/MathLib.h']:
 data=(a.source/rel).read_bytes();assert data==subprocess.check_output(['git','-C',str(a.source),'show',f'{pin}:{rel}']);sources[rel]=data

def method(s,name):
 start=s.index(name);opening=s.index('{',start);depth=0
 for end in range(opening,len(s)):
  depth+=(s[end]=='{')-(s[end]=='}')
  if depth==0:return s[start:end+1]
 raise ValueError(name)
spell=sources['Source/SpellcastingManager.cpp'].decode('utf-8-sig');frame=sources['Source/PhatSDK/Frame.cpp'].decode('utf-8-sig');math=sources['Source/PhatSDK/MathLib.cpp'].decode('utf-8-sig')
methods='\n'.join([method(math,'BOOL Vector::normalize_check_small('),method(math,'void Quaternion::normalize(')]+[method(frame,n)for n in ['void Frame::euler_set_rotate(','void Frame::set_rotate(','Vector Frame::get_vector_heading(','void Frame::set_vector_heading(','void Frame::cache()','Vector Frame::localtoglobalvec(']]+[method(spell,n)for n in ['Position CSpellcastingManager::GetSpellProjectileSpawnPosition(','Vector CSpellcastingManager::GetSpellProjectileSpawnVelocity(']])
harness=r'''
#include <cmath>
#include <cfloat>
#include <iomanip>
#include <iostream>
#include <algorithm>
using BOOL=int;constexpr bool TRUE=true,FALSE=false;constexpr float F_EPSILON=0.0002f;constexpr int GRAVITY_STATUS_BOOL=14;
#define DEG2RAD(x) ((x)*3.14159265358979323846/180.0)
#define RAD2DEG(x) ((x)*180.0/3.14159265358979323846)
#define EulGetOrd(ord,i,j,k,h,n,s,f) i=0;j=1;k=2;h=0;n=0;s=0;f=0
constexpr int EulFrmR=1,EulParOdd=1,EulRepYes=1;
using std::min;
struct Vector {float x,y,z;Vector(float a=0,float b=0,float c=0):x(a),y(b),z(c){}float magnitude()const{return (float)sqrt((x*x)+(y*y)+(z*z));}Vector operator+(Vector v)const{return Vector(x+v.x,y+v.y,z+v.z);}Vector operator-(Vector v)const{return Vector(x-v.x,y-v.y,z-v.z);}Vector operator*(float n)const{return Vector(x*n,y*n,z*n);}Vector operator/(float n)const{return Vector(x/n,y/n,z/n);}Vector&operator+=(Vector v){x+=v.x;y+=v.y;z+=v.z;return *this;}Vector&operator*=(float n){x*=n;y*=n;z*=n;return *this;}BOOL normalize_check_small();void normalize(){float n=1/magnitude();*this*=n;}};
struct Quaternion {float w=1,x=0,y=0,z=0;float magnitude()const{return (float)sqrt((x*x)+(y*y)+(z*z)+(w*w));}void normalize();};
struct Frame {Vector m_origin;Quaternion m_angles;float m00=1,m01=0,m02=0,m10=0,m11=1,m12=0,m20=0,m21=0,m22=1;void cache();void euler_set_rotate(Vector,int);void set_rotate(Quaternion);Vector get_vector_heading();void set_vector_heading(const Vector&);Vector localtoglobalvec(const Vector&)const;};
struct Position {Frame frame;Position add_offset(Vector v)const{Position result=*this;result.frame.m_origin+=v;return result;}Vector get_offset(Position other)const{return other.frame.m_origin-frame.m_origin;}Vector localtoglobalvec(Vector v)const{return frame.localtoglobalvec(v);}};
struct CWeenieObject {Position m_Position;float height=1.8f,radius=0.5f;Vector velocity;float GetHeight(){return height;}float GetRadius(){return radius;}Vector get_velocity(){return velocity;}};
struct CSpellProjectile {bool gravity=false;float GetRadius(){return .15f;}bool InqBoolQuality(int,bool){return gravity;}};
struct CSpellcastingManager {CWeenieObject source;CWeenieObject*GetCastSource(){return &source;}Position GetSpellProjectileSpawnPosition(CSpellProjectile*,CWeenieObject*,float*,double,bool);Vector GetSpellProjectileSpawnVelocity(Position*,CWeenieObject*,float,bool,bool,Vector*,double,bool);};
// METHODS
int main(){std::cout<<std::setprecision(9);for(bool gravity:{false,true})for(bool tracking:{false,true})for(bool self:{false,true})for(float heading:{0.0f,0.7f})for(int perturb:{0,1})for(int offset:{0,1}){
 CSpellcastingManager manager;manager.source.m_Position.frame.m_origin=Vector(1,2,0);manager.source.height=1.9f;manager.source.radius=.5f;manager.source.m_Position.frame.euler_set_rotate(Vector(0,0,heading),0);
 CWeenieObject target;target.m_Position.frame.m_origin=Vector(10,30,3);target.height=1.8f;target.velocity=Vector(3,-1,.5f);CWeenieObject*t=self?&manager.source:&target;CSpellProjectile projectile;projectile.gravity=gravity;float distance;auto position=manager.GetSpellProjectileSpawnPosition(&projectile,t,&distance,0,false);
 Vector create=position.localtoglobalvec(offset?Vector(.1f,.2f,-.1f):Vector());
 position=position.add_offset(perturb?Vector(-.5f*.2f,.25f*.3f,.75f*.4f):Vector());position=position.add_offset(create);
 auto velocity=manager.GetSpellProjectileSpawnVelocity(&position,t,20.0f,tracking,gravity,nullptr,0,false);
 auto v=position.frame.m_origin;std::cout<<gravity<<","<<tracking<<","<<self<<","<<heading<<","<<perturb<<","<<offset<<","<<v.x<<","<<v.y<<","<<v.z<<","<<velocity.x<<","<<velocity.y<<","<<velocity.z<<"\n";
}}
'''.replace('// METHODS',methods)
with tempfile.TemporaryDirectory(prefix='gdle-launch-')as td:
 b=Path(td);(b/'main.cpp').write_text(harness);subprocess.run(['c++','-std=c++17','-O0','-fno-fast-math','-ffp-contract=off',str(b/'main.cpp'),'-o',str(b/'main')],check=True);output=subprocess.check_output([str(b/'main')],text=True)
header=[f'# GDLE {pin}; AGPL-3.0-only, GDLE contributors']+[f'# sha256 {hashlib.sha256(v).hexdigest()} {k}'for k,v in sources.items()]+['# Original spawn/velocity/frame/math methods; Euler order0, single projectile, synthetic owned poses.']
(Path(__file__).parent.parent/'tests/fixtures/launch.csv').write_text('\n'.join(header)+'\n'+output)
