#!/usr/bin/env python3
"""Compile unchanged pinned GDLE MonsterAIManager::RollDiceCastSpell."""
import argparse,subprocess,tempfile,hashlib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);a=p.parse_args()
pin='353cbab52ef7da2b7063bc3e3f008461d8531693';rel='Source/MonsterAI.cpp';data=(a.source/rel).read_bytes()
assert data==subprocess.check_output(['git','-C',str(a.source),'show',f'{pin}:{rel}'])
s=data.decode('utf-8-sig');start=s.index('bool MonsterAIManager::RollDiceCastSpell()');opening=s.index('{',start);depth=0
for end in range(opening,len(s)):
 depth+=(s[end]=='{')-(s[end]=='}')
 if depth==0:break
method=s[start:end+1]
harness='''#include <map>
#include <vector>
#include <iostream>
#include <cstdint>
struct Timer{static inline double cur_time;};
struct Random{static inline std::vector<float> draws; static inline size_t index; static float RollDice(float,float){return draws.at(index++);}};
constexpr int AI_USE_MAGIC_DELAY_FLOAT=0;
struct Spell{float _casting_likelihood;};
struct Book{std::map<uint32_t,Spell> _spellbook;};
struct Quality{Book* _spell_book; float GetFloat(int,float){return 2;}};
struct Creature{Quality m_Qualities;};
struct MonsterAIManager{Creature* m_pWeenie; double m_fNextCastTime=0; uint32_t selected=0; bool RollDiceCastSpell();bool DoCastSpell(uint32_t id){selected=id;return true;}};
// METHOD
int main(){Book book{{{100,{0.2f}},{200,{0.8f}}}};Creature creature{{&book}};
for(float first:{0.0f,0.2f,0.3f,1.0f})for(float second:{0.0f,0.8f,0.9f,1.0f}){
MonsterAIManager ai{&creature};Random::draws={first,second};Random::index=0;Timer::cur_time=1;bool cast=ai.RollDiceCastSpell();
std::cout<<first<<","<<second<<","<<ai.selected<<","<<ai.m_fNextCastTime<<"\\n";
}}
'''.replace('// METHOD',method)
with tempfile.TemporaryDirectory(prefix='gdle-monster-oracle-') as td:
 b=Path(td);(b/'main.cpp').write_text(harness);subprocess.run(['c++','-std=c++17','-O0','-fno-fast-math','-ffp-contract=off',str(b/'main.cpp'),'-o',str(b/'main')],check=True);out=subprocess.check_output([str(b/'main')],text=True)
(Path(__file__).parent.parent/'tests/fixtures/spellcasting.csv').write_text(f'# GDLE {pin}; AGPL-3.0-only GDLE contributors\n# sha256 {hashlib.sha256(data).hexdigest()} {rel}\n# Verbatim selector; synthetic spellbook/clock/RNG adapters, no damage execution.\n'+out)
