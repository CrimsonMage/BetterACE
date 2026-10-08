#!/usr/bin/env python3
"""Execute verbatim pinned GDLE life-projectile source drain switch."""
import argparse,subprocess,tempfile,hashlib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);a=p.parse_args();pin='353cbab52ef7da2b7063bc3e3f008461d8531693';rel='Source/SpellcastingManager.cpp';data=(a.source/rel).read_bytes()
assert data==subprocess.check_output(['git','-C',str(a.source),'show',f'{pin}:{rel}'])
s=data.decode('utf-8-sig');start=s.index('switch (damageType)',s.index('float drainPercentage ='));opening=s.index('{',start);depth=0
for end in range(opening,len(s)):
 depth+=(s[end]=='{')-(s[end]=='}')
 if depth==0:break
branch=s[start:end+1]
source='''#include <cmath>
#include <algorithm>
#include <iostream>
enum Kind {HEALTH_DAMAGE_TYPE,STAMINA_DAMAGE_TYPE,MANA_DAMAGE_TYPE};
struct Source {int n;int GetHealth(){return n;}int GetStamina(){return n;}int GetMana(){return n;}void SetHealth(int v){n=v;}int Adjust(int d){int next=std::max(0,n+d);int change=next-n;n=next;return change;}int AdjustHealth(int d,bool){return Adjust(d);}int AdjustStamina(int d){return Adjust(d);}int AdjustMana(int d){return Adjust(d);}};
int main(){for(auto damageType:{HEALTH_DAMAGE_TYPE,STAMINA_DAMAGE_TYPE,MANA_DAMAGE_TYPE})for(int current:{1,5,15,50,16777217})for(float drainPercentage:{0.0f,0.1f,0.5f,1.0f}){Source src{current};auto pSource=&src;double selfDrainedAmount=0;
// SWITCH
std::cout<<int(damageType)<<","<<current<<","<<drainPercentage<<","<<int(selfDrainedAmount)<<","<<src.n<<"\\n";}}
'''.replace('// SWITCH',branch)
with tempfile.TemporaryDirectory(prefix='gdle-life-') as td:
 d=Path(td);(d/'main.cpp').write_text(source);subprocess.run(['c++','-std=c++17','-O0','-fno-fast-math','-ffp-contract=off',str(d/'main.cpp'),'-o',str(d/'main')],check=True);output=subprocess.check_output([str(d/'main')],text=True)
(Path(__file__).parent.parent/'tests/fixtures/life.csv').write_text(f'# GDLE {pin}; AGPL-3.0-only GDLE contributors\n# sha256 {hashlib.sha256(data).hexdigest()} {rel}\n# Verbatim life drain branch; explicit vital adapter, not impact modifiers.\n'+output)
