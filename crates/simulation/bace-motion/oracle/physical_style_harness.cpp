// Synthetic lookup/sequence adapters. Original methods inserted by generator.
#include <cstdint>
#include <map>
#include <vector>
#include <iostream>
#include <iomanip>
using BOOL=int;
constexpr BOOL TRUE=1,FALSE=0;
constexpr uint32_t CM_Style=0x80000000;
struct MotionData{uint32_t id,num_anims;uint8_t bitfield=0;};
template<class T>struct LongHash{std::map<uint32_t,T*> values;T* lookup(uint32_t key){auto i=values.find(key);return i==values.end()?nullptr:i->second;}};
template<class T>struct Lookup{std::map<uint32_t,T> values;BOOL lookup(uint32_t key,T*out){auto i=values.find(key);if(i==values.end())return FALSE;*out=i->second;return TRUE;}};
struct CSequence{std::vector<std::pair<uint32_t,float>> prepared;void clear_physics(){}void remove_cyclic_anims(){}};
struct MotionState{uint32_t style,substate;float substate_mod;void clear_modifiers(){}};
struct CMotionTable{uint32_t default_style=0x8000003d;Lookup<uint32_t> style_defaults;Lookup<LongHash<MotionData>*> links;LongHash<MotionData> cycles;MotionData*get_link(uint32_t,uint32_t,float,uint32_t,float);BOOL style(uint32_t,MotionState*,CSequence*,float,uint32_t*);void re_modify(CSequence*,MotionState*){}};
void add_motion(CSequence*s,MotionData*d,float speed){if(d)s->prepared.push_back({d->id,speed});}
// LINK
BOOL CMotionTable::style(uint32_t motionid,MotionState*curr_state,CSequence*sequence,float speed_mod,uint32_t*num_anims){
    *num_anims=0;MotionData*var_10=nullptr,*var_4=nullptr;uint32_t mtype2=curr_state->substate,new_substate=0;
// STYLE
    return FALSE;
}
int main(){std::cout<<std::setprecision(17);
    for(int scenario=0;scenario<9;++scenario)for(float speed:{-1.0f,.5f,1.0f,2.0f}){
        constexpr uint32_t from=0x80000049,to=0x80000040,ready=0x41000003,other=0x40000014;
        CMotionTable t;MotionState state{from,scenario==1||scenario==6?other:ready,scenario==6?-1.f:1.f};CSequence seq;
        t.style_defaults.values[from]=ready;t.style_defaults.values[to]=scenario==4?other:ready;t.style_defaults.values[t.default_style]=ready;
        MotionData a{11,2},b{12,1},c{13,3},d{14,scenario==7?3u:1u},e{15,2};
        std::map<uint32_t,LongHash<MotionData>> buckets;
        auto link=[&](uint32_t style,uint32_t current,uint32_t dest,MotionData*data){uint32_t key=(style<<16)|(current&0xffffff);buckets[key].values[dest]=data;t.links.values[key]=&buckets[key];};
        if(scenario!=2&&scenario!=3)link(from,ready,to,&a);
        if(scenario==1||scenario==6){link(from,other,ready,&b);link(from,ready,other,&b);}
        if(scenario==2){link(from,ready,t.default_style,&c);link(t.default_style,ready,to,&e);}
        t.cycles.values[(to<<16)|(ready&0xffffff)]=&d;
        t.cycles.values[(from<<16)|(ready&0xffffff)]=&d;
        if(scenario==8)t.cycles.values.erase((to<<16)|(ready&0xffffff));
        uint32_t n=99;BOOL ok=t.style(scenario==5?from:to,&state,&seq,speed,&n);
        std::cout<<scenario<<","<<speed<<","<<ok<<","<<state.style<<","<<state.substate<<","<<state.substate_mod<<","<<n;
        for(auto [id,rate]:seq.prepared)std::cout<<","<<id<<":"<<rate;
        std::cout<<"\n";
    }
}
