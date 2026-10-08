#!/usr/bin/env python3
"""Compile unchanged GDLE source-item skill method and cloak override statements."""
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
s=read('Source/SpellcastingManager.cpp');projectile=read('Source/SpellProjectile.cpp');override=method(projectile,'if (m_CachedSpellCastData.spell_id == 1783')
program=r'''
#include <cstdint>
#include <map>
#include <algorithm>
#include <iostream>
using std::max;constexpr bool FALSE=false;using STypeSkill=uint32_t;constexpr int ITEM_SPELLCRAFT_INT=106,WIELD_DIFFICULTY_INT=160,CREATURE_ENCHANTMENT_SKILL=31,ITEM_ENCHANTMENT_SKILL=32,LIFE_MAGIC_SKILL=33,WAR_MAGIC_SKILL=34,VOID_MAGIC_SKILL=43;
#define SERVER_ERROR std::cerr
struct Q{std::map<int,int>values;bool InqInt(int k,int&v,bool=false,bool=false){auto it=values.find(k);if(it==values.end())return false;v=it->second;return true;}};
struct CWeenieObject {Q m_Qualities;bool cloak=true;bool IsCloak(){return cloak;}void InqSkill(uint32_t k,uint32_t&v,bool){v=k==34?111:k==33?222:k==31?333:k==32?444:555;}};
struct World{CWeenieObject*item;CWeenieObject*FindObject(uint32_t){return item;}}world;World*g_pWorld=&world;
struct Spell{uint32_t skill;uint32_t InqSkillForSpell(){return skill;}};
struct Data{uint32_t caster_id=1,source_id=2,spell_id=1783,current_skill=0;Spell*spell=nullptr;};
struct CSpellcastingManager{Data m_SpellCastData;CWeenieObject*m_pWeenie;uint32_t DetermineSkillLevelForSpell();};
// METHOD
int main(){for(int spellcraft:{-1,0,300})for(uint32_t school:{0,31,32,33,34,43}){CWeenieObject item;world.item=&item;if(spellcraft>=0)item.m_Qualities.values[106]=spellcraft;Spell spell{school};CSpellcastingManager manager;manager.m_pWeenie=&item;manager.m_SpellCastData.spell=&spell;std::cout<<"skill,"<<spellcraft<<","<<school<<","<<manager.DetermineSkillLevelForSpell()<<"\n";}
for(uint32_t spell:{100,1783,1789,5331})for(int difficulty:{-1,0,150,300})for(bool cloak:{false,true}){CWeenieObject item;world.item=&item;item.cloak=cloak;if(difficulty>=0)item.m_Qualities.values[160]=difficulty;Data m_CachedSpellCastData;m_CachedSpellCastData.spell_id=spell;m_CachedSpellCastData.current_skill=77;
// OVERRIDE
std::cout<<"cloak,"<<spell<<","<<difficulty<<","<<cloak<<","<<m_CachedSpellCastData.current_skill<<"\n";}}
'''.replace('// METHOD',method(s,'uint32_t CSpellcastingManager::DetermineSkillLevelForSpell()')).replace('// OVERRIDE',override)
with tempfile.TemporaryDirectory(prefix='gdle-proc-skill-')as td:
 b=Path(td);(b/'main.cpp').write_text(program);subprocess.run(['c++','-std=c++17','-O0',str(b/'main.cpp'),'-o',str(b/'main')],check=True);out=subprocess.check_output([str(b/'main')],text=True)
header=[f'# GDLE {pin}; AGPL-3.0-only GDLE contributors']+[f'# sha256 {hashlib.sha256(v).hexdigest()} {k}'for k,v in sources.items()]+['# Original DetermineSkillLevelForSpell and cloak Life projectile override; explicit owned item qualities and distinct school projections.']
Path(__file__).parent.parent.joinpath('tests/fixtures/proc_skill.csv').write_text('\n'.join(header)+'\n'+out)
