#!/usr/bin/env python3
"""Original GDLE transfer arithmetic, AdjustHealth and natural resistance."""
import argparse,subprocess,tempfile,hashlib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);a=p.parse_args();pin='353cbab52ef7da2b7063bc3e3f008461d8531693';sources={}
def read(rel):
 d=(a.source/rel).read_bytes();assert d==subprocess.check_output(['git','-C',str(a.source),'show',f'{pin}:{rel}']);sources[rel]=d;return d.decode('utf-8-sig')
def method(s,n):
 start=s.index(n);op=s.index('{',start);depth=0
 for end in range(op,len(s)):
  depth+=(s[end]=='{')-(s[end]=='}')
  if depth==0:return s[start:end+1]
 raise ValueError(n)
s=read('Source/SpellcastingManager.cpp');w=read('Source/WeenieObject.cpp');u=read('Source/Util.cpp')
transfer=s[s.index('int sourceTakeAmount ='):s.index('\n\tif (sourceResultValue != sourceStartValue)')]
natural=s[s.index('\t\t\tfloat strAndEnd ='):s.index('\n\t\t\tif (resistanceNatural < drainResistMod)')]
program=r'''
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <iostream>
#include <iomanip>
using std::max;
enum {HEALING_BOOST_RATING_INT,HEALING_RESIST_RATING_INT,LIFE_RESIST_RATING_INT};
// RATING
class CWeenieObject{public:int health=0,maximum=100,rating=0;int GetRating(int key){return key==HEALING_BOOST_RATING_INT?rating:0;}int GetMaxHealth(){return maximum;}int GetHealth(){return health;}void SetHealth(int h){health=h;}int AdjustHealth(int,bool=true);};
// HEALTH
struct Meta{float _proportion,_lossPercent;int _transferCap;};
int main(){std::cout<<std::setprecision(17);
for(uint32_t sourceStartValue:{0,1,50,100})for(uint32_t destStartValue:{0,90,100})for(float proportion:{0.f,.25f,.66f,1.f})for(float loss:{0.f,.1f,1.f})for(int cap:{0,25})for(double drainResistMod:{.5,1.,1.5})for(double boostResistMod:{.5,1.,1.5}){
 uint32_t destMaxValue=100;Meta m{proportion,loss,cap};Meta*meta=&m;
 // TRANSFER
 std::cout<<"transfer,"<<sourceStartValue<<","<<destStartValue<<","<<proportion<<","<<loss<<","<<cap<<","<<drainResistMod<<","<<boostResistMod<<","<<sourceTakeAmount<<","<<destGiveAmount<<"\n";
}
for(int current:{0,50,99,100})for(double amount:{0.9,1.5,20.9,99.9})for(int rating:{-30,0,20}){
 CWeenieObject actor;actor.health=current;actor.rating=rating;actor.AdjustHealth(amount);
 std::cout<<"heal,"<<current<<","<<amount<<","<<GetRatingMod(rating)<<","<<actor.health<<"\n";
}
for(uint32_t strength:{0,99,100,101,300,500})for(uint32_t endurance:{0,99,100,101,300,500}){
 // NATURAL
 std::cout<<"natural,"<<strength<<","<<endurance<<","<<resistanceNatural<<"\n";
}}
'''.replace('// RATING',method(u,'float GetRatingMod(')).replace('// HEALTH',method(w,'int CWeenieObject::AdjustHealth(')).replace('// TRANSFER',transfer).replace('// NATURAL',natural)
with tempfile.TemporaryDirectory(prefix='gdle-vitals-')as td:
 b=Path(td);(b/'main.cpp').write_text(program);subprocess.run(['c++','-std=c++17','-O0','-fno-fast-math','-ffp-contract=off',str(b/'main.cpp'),'-o',str(b/'main')],check=True);out=subprocess.check_output([str(b/'main')],text=True)
header=[f'# GDLE {pin}; AGPL-3.0-only, GDLE contributors']+[f'# sha256 {hashlib.sha256(v).hexdigest()} {k}'for k,v in sources.items()]+['# Original TransferVital scalar block, AdjustHealth and natural-resistance branch. Unchecked source overdraw recorded; production rejects atomically. Original negative-rating multiplier supplied explicitly; simulation uses reviewed correction.']
Path(__file__).parent.parent.joinpath('tests/fixtures/vitals.csv').write_text('\n'.join(header)+'\n'+out)
