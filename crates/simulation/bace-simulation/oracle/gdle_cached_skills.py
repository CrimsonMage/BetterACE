#!/usr/bin/env python3
"""Execute pinned original AllegianceTreeNode::UpdateWithWeenie with value-only adapters.
GDLE Source/AllegianceManager.cpp; upstream source remains outside this repository.
"""
import argparse, pathlib, subprocess, tempfile, hashlib, csv
p=argparse.ArgumentParser();p.add_argument('checkout',type=pathlib.Path);a=p.parse_args()
pin='353cbab52ef7da2b7063bc3e3f008461d8531693'
assert subprocess.check_output(['git','-C',str(a.checkout),'rev-parse','HEAD'],text=True).strip()==pin
source=(a.checkout/'Source/AllegianceManager.cpp').read_text()
start=source.index('void AllegianceTreeNode::UpdateWithWeenie(');end=source.index('\nDEFINE_PACK(AllegianceTreeNode)',start)
body=source[start:end]
harness=r'''#include <cstdint>
#include <string>
#include <iostream>
#define FALSE false
enum HeritageGroup{Invalid_HeritageGroup=0}; enum Gender{Invalid_Gender=0};
enum {HERITAGE_GROUP_INT=1,GENDER_INT=2,LEVEL_INT=3,LEADERSHIP_SKILL=35,LOYALTY_SKILL=36};
struct Qualities{uint32_t raw_l,current_l,raw_y,current_y;int raw_reads=0;bool InqSkill(int key,uint32_t&v,bool raw){raw_reads+=raw;v=key==35?(raw?raw_l:current_l):(raw?raw_y:current_y);return true;}};
struct CWeenieObject{Qualities m_Qualities;uint32_t level;uint32_t GetID(){return 1;}std::string GetName(){return "Alice";}uint32_t InqIntQuality(int key,int fallback){return key==LEVEL_INT?level:1;}};
struct AllegianceTreeNode{uint32_t _charID,_level,_leadership,_loyalty;std::string _charName;HeritageGroup _hg;Gender _gender;void UpdateWithWeenie(CWeenieObject*);};
'''+body+r'''
int main(){CWeenieObject w;while(std::cin>>w.level>>w.m_Qualities.raw_l>>w.m_Qualities.current_l>>w.m_Qualities.raw_y>>w.m_Qualities.current_y){AllegianceTreeNode n;n.UpdateWithWeenie(&w);std::cout<<n._level<<","<<n._leadership<<","<<n._loyalty<<","<<w.m_Qualities.raw_reads<<"\n";}}
'''
rows=[(25,100,101,100,100),(25,100,777,100,120),(275,291,999,291,999),(1,0,0,0,0)]
with tempfile.TemporaryDirectory(prefix='bace-gdle-cache-') as directory:
    directory=pathlib.Path(directory);(directory/'main.cpp').write_text(harness)
    subprocess.run(['g++','-std=c++17','-O0',str(directory/'main.cpp'),'-o',str(directory/'oracle')],check=True)
    results=subprocess.check_output([str(directory/'oracle')],input=''.join(' '.join(map(str,r))+'\n' for r in rows),text=True).splitlines()
output=pathlib.Path(__file__).parents[1]/'src/kernel/allegiance_cache/fixtures/gdle_cached_skills.csv'
with output.open('w') as f:
    f.write(f'# GDLE {pin} AllegianceTreeNode::UpdateWithWeenie sha256={hashlib.sha256(body.encode()).hexdigest()}\n')
    writer=csv.writer(f,lineterminator='\n');writer.writerow(('level','raw_leadership','current_leadership','raw_loyalty','current_loyalty','cached_level','cached_leadership','cached_loyalty','raw_reads'))
    for row,result in zip(rows,results):writer.writerow((*row,*result.split(',')))
print(f'Wrote {len(rows)} original C++ vectors to {output}')
