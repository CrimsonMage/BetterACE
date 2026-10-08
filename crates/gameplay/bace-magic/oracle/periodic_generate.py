#!/usr/bin/env python3
"""Original GDLE periodic aggregation call ordering and source selection.
Enchantment's scalar/category computation is independently qualified elsewhere;
this harness supplies its explicit resulting values and records actual calls.
"""
import argparse,subprocess,tempfile,hashlib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);a=p.parse_args();pin='353cbab52ef7da2b7063bc3e3f008461d8531693';rel='Source/WeenieObject.cpp';data=(a.source/rel).read_bytes();assert data==subprocess.check_output(['git','-C',str(a.source),'show',f'{pin}:{rel}']);s=data.decode('utf-8-sig')
def method(name):
 start=s.index(name);op=s.index('{',start);depth=0
 for end in range(op,len(s)):
  depth+=(s[end]=='{')-(s[end]=='}')
  if depth==0:return s[start:end+1]
 raise ValueError(name)
program=r'''
#include <cstdint>
#include <vector>
#include <string>
#include <map>
#include <iostream>
enum{DamageOverTime_SpellIndex=0x10000,DF_Bleed_Damage_SpellCategory=685,NetherDamageOverTime_Raising_SpellCategory=636,NetherDamageOverTime_Raising2_SpellCategory=637,NetherDamageOverTime_Raising3_SpellCategory=638,AetheriaProcDamageOverTime_Raising_SpellCategory=631,HealOverTime_Raising_SpellCategory=617,AetheriaProcHealthOverTime_Raising_SpellCategory=630,DAMAGE_OVER_TIME_INT=318,NETHER_OVER_TIME_INT=330,HEAL_OVER_TIME_INT=312,BASE_DAMAGE_TYPE=0x10000000,NETHER_DAMAGE_TYPE=1024,PS_DirtyFightingDamageOverTime=1,PS_HealthDownVoid=2,LTT_COMBAT=0};
struct CSpellBase{int _bitfield=0,_category=0;std::string _name="fixture";};struct CSpellTable{std::map<int,CSpellBase>spells;const CSpellBase*GetSpellBase(int id){return spells.count(id)?&spells[id]:nullptr;}}table;
struct MagicSystem{static CSpellTable*GetSpellTable(){return &table;}};struct Config{bool ShowDotSpells(){return false;}double VoidDamageReduction(){return 1;}}config;Config*g_pConfig=&config;
struct Enchantment{int _id,_spell_category;uint32_t _caster;};struct Registry{std::vector<Enchantment>*_add_list;};
struct Quality{Registry*_enchantment_reg;int damage=0,nether=11,healing=0;bool EnchantInt(int key,int&value,bool){value=key==318?damage:key==330?nether:healing;return true;}};
class CWeenieObject;struct DamageEventData{double baseDamage=0;int damage_type=0,outputDamageFinal=0;CWeenieObject*source=nullptr,*target=nullptr;bool isDot=false,killingBlow=false;};
struct Physics{void EmitEffect(int,double){}}physics;
template<class...A>std::string csprintf(const char*,A...){return {};}
class CWeenieObject{public:int id=1,heals=0,totalHeal=0;Quality m_Qualities;Physics*_phys_obj=&physics;std::vector<DamageEventData>damage;CWeenieObject*AsPlayer(){return nullptr;}CWeenieObject*GetWorldTopLevelOwner(){return this;}void SendText(std::string,int){}void TakeDamage(DamageEventData&d){damage.push_back(d);}int AdjustHealth(int amount,bool){++heals;totalHeal+=amount;return amount;}void CheckForTickingDots();void CheckForTickingHots();};
struct World{std::map<int,CWeenieObject*>actors;CWeenieObject*FindObject(uint32_t id){return actors.count(id)?actors[id]:nullptr;}}world;World*g_pWorld=&world;
// METHODS
int main(){for(int earlierStronger:{0,1})for(int present:{0,1})for(int healCount:{1,2,3}){
 CWeenieObject target,source2,source3;source2.id=2;source3.id=3;world.actors.clear();if(present){world.actors[2]=&source2;world.actors[3]=&source3;}
 std::vector<Enchantment>entries{{100,685,2},{101,685,3},{102,631,2},{103,636,2},{104,637,3}};table.spells={{100,{0x10000,685}},{101,{0x10000,685}},{102,{0,631}},{103,{0x10000,636}},{104,{0x10000,637}}};
 for(int i=0;i<healCount;i++){int category=i%2==0?617:630;entries.push_back({200+i,category,2});table.spells[200+i]={0,category};}
 Registry reg{&entries};target.m_Qualities._enchantment_reg=&reg;target.m_Qualities.damage=(earlierStronger?10:20)+5;target.m_Qualities.healing=healCount==1?3:healCount==2?7:11;
 target.CheckForTickingDots();target.CheckForTickingHots();
 std::cout<<earlierStronger<<","<<present<<","<<healCount;for(auto&d:target.damage)std::cout<<","<<d.outputDamageFinal<<","<<(d.source?d.source->id:0);std::cout<<","<<target.heals<<","<<target.totalHeal<<"\n";
}}
'''.replace('// METHODS',method('void CWeenieObject::CheckForTickingDots()')+'\n'+method('void CWeenieObject::CheckForTickingHots()'))
with tempfile.TemporaryDirectory(prefix='gdle-periodic-')as td:
 b=Path(td);(b/'main.cpp').write_text(program);subprocess.run(['c++','-std=c++17','-O0',str(b/'main.cpp'),'-o',str(b/'main')],check=True);out=subprocess.check_output([str(b/'main')],text=True)
Path(__file__).parent.parent.joinpath('tests/fixtures/periodic_native.csv').write_text(f'# GDLE {pin}; AGPL-3.0-only GDLE contributors\n# sha256 {hashlib.sha256(data).hexdigest()} {rel}\n# Unchanged CheckForTickingDots/Hots; explicit EnchantInt results (separate scalar oracle), accepted registry order and source lookup. Records source aggregation and repeated HoT calls, no copied Rust calculation.\n'+out)
