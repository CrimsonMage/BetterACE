#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <iomanip>
#include <type_traits>
template<class A,class B> auto max(A a,B b){using T=std::common_type_t<A,B>;return std::max<T>(a,b);}
template<class A,class B> auto min(A a,B b){using T=std::common_type_t<A,B>;return std::min<T>(a,b);}
// ATTACK_ENUM
constexpr auto Motion_DualWieldCombat=0x80000046u,Motion_SwordShieldCombat=0x80000040u;
constexpr auto Motion_AttackLow1=0x10000064u,Motion_AttackMed1=0x10000063u,Motion_AttackHigh1=0x10000062u;
constexpr auto Motion_AttackLow4=0x10000188u,Motion_AttackMed4=0x10000187u,Motion_AttackHigh4=0x10000186u;
constexpr int ATTACK_TYPE_INT=1,DEFAULT_COMBAT_STYLE_INT=2,Unarmed_CombatStyle=1,LOW_ATTACK_HEIGHT=3,MEDIUM_ATTACK_HEIGHT=2,HIGH_ATTACK_HEIGHT=1,QUICKNESS_ATTRIBUTE=1,FALSE=0,COMBAT_USE_OFFHAND=2;
struct Qualities{uint32_t attack=0,style=0,quick=100;uint32_t GetInt(int,int=0){return style;}void InqAttribute(int,uint32_t& q,int){q=quick;}};
struct CombatManeuver{uint32_t motion;};
struct Table{bool available=true;uint32_t selected=0;CombatManeuver value;CombatManeuver* TryGetCombatManuever(uint32_t,AttackType a,int){selected=a;value.motion=0x10001000+a;return available?&value:nullptr;}};
struct Motion{uint32_t style=Motion_SwordShieldCombat;uint32_t InqStyle(){return style;}};
struct CWeenieObject{Qualities m_Qualities;Table table;Table* _combatTable=&table;Motion motion;CWeenieObject* weapon=this;CWeenieObject* offhand=this;uint32_t attacktime=0;uint32_t GetID(){return 1;}Motion* get_minterp(){return &motion;}CWeenieObject* GetWieldedCombat(int style){return style==COMBAT_USE_OFFHAND?offhand:weapon;}int InqIntQuality(int,int){return m_Qualities.attack;}int GetAttackTimeUsingWielded(){return attacktime;}};
struct CAttackEventData{void Setup(){}};
struct CMeleeAttackEvent:CAttackEventData{CWeenieObject* _weenie;uint32_t _do_attack_animation=0;int _combat_style=0,_attack_height=1;float _attack_power=0,_attack_speed=0;bool m_bCanCharge=false;void Cancel(){};void Setup();};
struct CDualWieldAttackEvent:CMeleeAttackEvent{uint32_t _main_attack_motion=0,_offhand_attack_motion=0;void Setup();};
// SETUP
// DUAL_SETUP
// IMBUE
// RATING
// SKILL
uint32_t bits(float v){uint32_t n;memcpy(&n,&v,4);return n;}
int main(){std::cout<<std::setprecision(17);
 for(auto style:{Motion_SwordShieldCombat,Motion_DualWieldCombat})for(auto attack:{1u,2u,4u,6u,8u,0xa0u,0x140u,0x80u})for(int height:{1,2,3})for(float power:{0.f,0.249999f,0.25f,0.749999f,0.75f,1.f})for(bool available:{false,true}){
 CWeenieObject actor,weapon;actor.weapon=&weapon;actor.offhand=&weapon;actor.motion.style=style;actor.table.available=available;weapon.m_Qualities.attack=attack;weapon.m_Qualities.style=attack==1?1:2;
 CDualWieldAttackEvent e;e._weenie=&actor;e._attack_height=height;e._attack_power=power;e.Setup();std::cout<<"maneuver,"<<style<<','<<attack<<','<<height<<','<<power<<','<<available<<','<<e._main_attack_motion<<','<<e._offhand_attack_motion<<','<<bits(e._attack_speed)<<'\n';}
 for(double skill:{0,124,125,149,150,151,250,359,360,399,400,401})for(double base:{0.1,1.0})for(double cap:{0.5,2.5,7.0})std::cout<<"imbue,"<<skill<<','<<base<<','<<cap<<','<<GetImbueMultiplier(skill,150,400,base,cap,false)<<'\n';
 for(int rating:{-101,-100,-99,-50,-10,0,10,100})std::cout<<"rating,"<<rating<<','<<GetRatingMod(rating)<<'\n';
 for(int offense:{0,100,200,300,500})for(int defense:{0,100,200,300,500})std::cout<<"skill,"<<offense<<','<<defense<<','<<GetSkillChance(offense,defense)<<'\n';
}
