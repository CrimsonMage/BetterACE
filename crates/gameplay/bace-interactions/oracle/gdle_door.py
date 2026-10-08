#!/usr/bin/env python3
"""Compile verbatim pinned GDLE set_ethereal with synthetic overlap/cell adapters."""
import argparse,hashlib,subprocess,tempfile
from pathlib import Path
PIN='353cbab52ef7da2b7063bc3e3f008461d8531693'
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);a=p.parse_args()
if subprocess.check_output(['git','-C',str(a.source),'rev-parse','HEAD'],text=True).strip()!=PIN:raise ValueError('wrong GDLE pin')
rel='Source/PhatSDK/PhysicsObj.cpp';data=(a.source/rel).read_bytes()
if data!=subprocess.check_output(['git','-C',str(a.source),'show',f'{PIN}:{rel}']):raise ValueError('modified reference')
s=data.decode('utf-8-sig');start=s.index('int CPhysicsObj::set_ethereal(');opening=s.index('{',start);depth=0
for i in range(opening,len(s)):
 depth+=(s[i]=='{')-(s[i]=='}')
 if depth==0:method=s[start:i+1];break
harness='''#include <iostream>
constexpr unsigned ETHEREAL_PS=1,CHECK_ETHEREAL_TS=2;
class CPhysicsObj{public:unsigned m_PhysicsState=0,transient_state=0;bool parent=false,cell=true,overlap=false;bool ethereal_check_for_collisions(){return overlap;}int set_ethereal(int ethereal,int send_event);};
// METHOD
int main(){for(unsigned initial=0;initial<4;initial++)for(int ethereal=0;ethereal<2;ethereal++)for(int occupied=0;occupied<2;occupied++){CPhysicsObj p;p.m_PhysicsState=initial&1;p.transient_state=initial&2;p.overlap=occupied;int result=p.set_ethereal(ethereal,0);std::cout<<initial<<","<<ethereal<<","<<occupied<<","<<(p.m_PhysicsState&1)<<","<<((p.transient_state&2)?1:0)<<","<<result<<"\\n";}}
'''.replace('// METHOD',method)
with tempfile.TemporaryDirectory(prefix='bace-door-oracle-') as td:
 b=Path(td);(b/'oracle.cpp').write_text(harness);subprocess.run(['c++','-std=c++17','-O0',str(b/'oracle.cpp'),'-o',str(b/'oracle')],check=True);output=subprocess.check_output([str(b/'oracle')],text=True)
 dest=Path(__file__).resolve().parents[1]/'tests/fixtures/gdle-solidity.csv';dest.parent.mkdir(parents=True,exist_ok=True);dest.write_text(f'# GDLE {PIN}; AGPL-3.0-only; GDLE contributors\n# sha256 {hashlib.sha256(data).hexdigest()} {rel}\n# Synthetic caller: unparented, loaded cell; no client authority\n'+output)
