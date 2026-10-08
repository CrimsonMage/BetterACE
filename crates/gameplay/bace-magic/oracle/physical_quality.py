#!/usr/bin/env python3
"""Compile unchanged GDLE scalar/detail enchanting methods with synthetic lists."""
from pathlib import Path
import tempfile,subprocess,hashlib
ROOT=Path(__file__).resolve().parents[4];source=Path('/tmp/ace-gdle-research-353cbab');pin='353cbab52ef7da2b7063bc3e3f008461d8531693'
def read(name):
 b=(source/name).read_bytes();assert b==subprocess.check_output(['git','-C',str(source),'show',f'{pin}:{name}']);return b.decode('utf-8-sig')
def method(text,signature):
 start=text.index(signature);a=text.index('{',start);depth=1;b=a+1
 while depth:depth+=(text[b]=='{')-(text[b]=='}');b+=1
 return text[start:b]
s=read('Source/PhatSDK/Qualities.cpp');h=read('Source/PhatSDK/Qualities.h')
methods='\n'.join(method(s,x) for x in ['BOOL Enchantment::Enchant(float *value)','BOOL Enchantment::Enchant(EnchantedQualityDetails *value)','BOOL CEnchantmentRegistry::Enchant(PackableListWithJson<Enchantment> *affecting, float *new_value)','BOOL CEnchantmentRegistry::EnchantInt(unsigned int stype','BOOL CEnchantmentRegistry::EnchantFloat(unsigned int stype'])
program=r'''
#include <vector>
#include <algorithm>
#include <iostream>
#include <bit>
#include <cstdint>
using BOOL=int;const int TRUE=1,FALSE=0,Additive_EnchantmentType=0x8000,Multiplicative_EnchantmentType=0x4000,Int_EnchantmentType=4,Float_EnchantmentType=8;
DETAILS;
struct Enchantment{struct{unsigned type;float val;}_smod;BOOL Enchant(float*);BOOL Enchant(EnchantedQualityDetails*);};
template<class T>using PackableListWithJson=std::vector<T>;
struct ACQualityFilter{bool QueryInt(unsigned){return true;}bool QueryFloat(unsigned){return true;}};ACQualityFilter filter;ACQualityFilter*CachedEnchantableFilter=&filter;
struct CEnchantmentRegistry{PackableListWithJson<Enchantment>*_mult_list,*_add_list;BOOL EnchantInt(unsigned,int*,BOOL);BOOL EnchantFloat(unsigned,double*);BOOL Enchant(PackableListWithJson<Enchantment>*,float*);void CullEnchantmentsFromList(PackableListWithJson<Enchantment>*list,unsigned,unsigned,PackableListWithJson<Enchantment>*out){out->insert(out->end(),list->begin(),list->end());}};
METHODS
int main(){for(int raw:{0,1,7,10000,16777217})for(int sign:{1,-1}){PackableListWithJson<Enchantment>mult={{{0x4000,1.2f}},{{0x4000,.7f}}},add={{{0x8000,float(sign*13)}},{{0x8000,-4.f}}};CEnchantmentRegistry r{&mult,&add};int integer=raw;r.EnchantInt(44,&integer,false);double scalar=raw;r.EnchantFloat(62,&scalar);EnchantedQualityDetails details;details.rawValue=raw;for(auto&e:mult)e.Enchant(&details);for(auto&e:add)e.Enchant(&details);std::cout<<raw<<","<<sign<<","<<integer<<","<<std::bit_cast<uint64_t>(scalar)<<","<<std::bit_cast<uint64_t>(details.valueIncreasingMultiplier)<<","<<std::bit_cast<uint64_t>(details.valueDecreasingMultiplier)<<","<<std::bit_cast<uint64_t>(details.valueIncreasingAdditive)<<","<<std::bit_cast<uint64_t>(details.valueDecreasingAdditive)<<"\n";}}
'''.replace('DETAILS',method(h,'struct EnchantedQualityDetails')).replace('METHODS',methods)
with tempfile.TemporaryDirectory(prefix='gdle-quality-oracle-') as tmp:
 p=Path(tmp);(p/'oracle.cpp').write_text(program);subprocess.run(['g++','-std=c++20','-O0','-ffp-contract=off',str(p/'oracle.cpp'),'-o',str(p/'oracle')],check=True);output=subprocess.check_output([str(p/'oracle')],text=True);(Path(__file__).resolve().parents[1]/'tests/fixtures/physical_quality.csv').write_text('# GDLE '+pin+' Qualities.cpp sha256 '+hashlib.sha256((source/'Source/PhatSDK/Qualities.cpp').read_bytes()).hexdigest()+'\n'+output)
