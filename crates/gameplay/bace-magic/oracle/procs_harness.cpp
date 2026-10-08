#include <cstdint>
#include <map>
#include <vector>
#include <string>
#include <iostream>
#include <iomanip>
#include <algorithm>
using std::min;
using std::max;
enum {SIGIL_ONE_LOC=1,SIGIL_TWO_LOC=2,SIGIL_THREE_LOC=3,CLOAK_LOC=4,PROC_SPELL_DID=55,CLOAK_WEAVE_PROC_INT=352,CURRENT_ENEMY_IID=9,LTT_MAGIC=0,LTT_MAGIC_CASTING_CHANNEL=0};
using BOOL=int;constexpr int TRUE=1,FALSE=0;
// ATTRIBUTE ENUM
// SCHOOL ENUM
struct SecondaryAttribute {uint32_t _init_level=100,_level_from_cp=0,_current=100;};
struct AttributeCache {SecondaryAttribute h,s,m;SecondaryAttribute *_health=&h,*_stamina=&s,*_mana=&m;BOOL InqAttribute2nd(STypeAttribute2nd,uint32_t&);};
// ATTRIBUTE GETTER
struct Qualities {AttributeCache attributes;int level=1,kind=0;uint32_t proc=0,health=100;bool enemy=false;int enemyId=99;bool InqDataID(int,uint32_t&v){v=proc;return proc!=0;}bool InqInt(int,int&v){v=kind;return kind!=0;}uint32_t GetDID(int,uint32_t){return proc;}void InqAttribute2nd(STypeAttribute2nd key,uint32_t&v,bool){attributes.InqAttribute2nd(key,v);}bool InqInstanceID(int,uint32_t&v){v=enemyId;return enemy;}};
struct Cast {int item;uint32_t target,spell;};std::vector<Cast> calls;
struct CSpellcastingManager {int item=0;void CastSpellInstant(uint32_t target,uint32_t spell){calls.push_back({item,target,spell});}};
struct CSpellBase {std::string _name="fixture";};struct CSpellBaseEx{uint32_t _bitfield=0;};
struct CSpellTable {CSpellBase s;CSpellBase*GetSpellBase(uint32_t){return &s;}};struct CSpellTableEx{std::map<int,CSpellBaseEx> spells;const CSpellBaseEx*GetSpellBase(int id){auto i=spells.find(id);return i==spells.end()?nullptr:&i->second;}};
struct Portal {CSpellTableEx table;CSpellTableEx*GetSpellTableEx(){return &table;}} portal;Portal*g_pPortalDataEx=&portal;
struct MagicSystem{static CSpellTable*GetSpellTable(){static CSpellTable table;return &table;}};
struct Config{float GetBlueSigilProcRate(){return .005f;}float GetYellowSigilProcRate(){return .0075f;}float GetRedSigilProcRate(){return .01f;}float GetCloakBaseProcRate(){return .05f;}float GetCloakHalfHealthProcRate(){return .075f;}float GetCloakQuarterHealthProcRate(){return .05f;}float GetCloakTenthHealthProcRate(){return .025f;}float GetCloakPerLevelProcRate(){return .01f;}}config;Config*g_pConfig=&config;
struct Random{static double roll;static double RollDice(double,double){return roll;}};double Random::roll=0;
template<class...A>std::string csprintf(const char*,A...){return {};}
struct DamageEventData{int outputDamageFinal;bool isPvP;};
class CWeenieObject{public:int id=0;Qualities m_Qualities;CSpellcastingManager manager;std::map<int,CWeenieObject*>worn;CWeenieObject*GetWielded(int slot){return worn.count(slot)?worn[slot]:nullptr;}int GetLevel(){return m_Qualities.level;}uint32_t GetID(){return id;}std::string GetName(){return "fixture";}bool _IsPlayer(){return true;}void SendText(std::string,int){}CSpellcastingManager*MakeSpellcastingManager(){manager.item=id;return &manager;}void Cloak(DamageEventData&damageData){
// CLOAK
}};
class CPlayerWeenie:public CWeenieObject{public:CWeenieObject*procSigil=nullptr;std::map<int,double>_sigilProcs;bool HasSigils(){return !worn.empty();}std::map<int,double>GetSigilProcRate(CWeenieObject*&);void HandleAetheriaProc(uint32_t);};
// METHODS
int main(){std::cout<<std::setprecision(17);
std::cout<<"school,"<<War_Magic<<",0\n"<<"school,"<<Life_Magic<<",1\n"<<"school,"<<CreatureEnchantment_Magic<<",2\n"<<"school,"<<ItemEnchantment_Magic<<",3\n"<<"school,"<<Void_Magic<<",4\n";
for(int health:{40,100})for(int damage:{20,49,50,99,100,200})for(int pvp:{0,1})for(int level:{1,5})for(int kind:{1,2})for(int spell:{100,5754})for(int enemy:{0,1})for(double roll:{0.,.05,.1,.2}){
 CWeenieObject owner,cloak;owner.id=1;owner.m_Qualities.attributes.h._current=health;cloak.id=10;cloak.m_Qualities.level=level;cloak.m_Qualities.kind=kind;cloak.m_Qualities.proc=spell;owner.m_Qualities.enemy=enemy;owner.worn[CLOAK_LOC]=&cloak;DamageEventData d{damage,bool(pvp)};Random::roll=roll;calls.clear();owner.Cloak(d);
 std::cout<<"cloak,"<<health<<","<<damage<<","<<pvp<<","<<level<<","<<kind<<","<<spell<<","<<enemy<<","<<roll<<","<<d.outputDamageFinal<<","<<calls.size()<<","<<(calls.empty()?0:calls[0].target)<<"\n";
}
for(int mask:{1,3,7})for(int duplicate:{0,1})for(int self:{0,1})for(int target:{0,99})for(double roll:{0.,.02,.1}){
 CPlayerWeenie owner;owner.id=1;CWeenieObject items[3];portal.table.spells.clear();for(int i=0;i<3;i++)if(mask&(1<<i)){items[i].id=10+i;items[i].m_Qualities.level=i+1;items[i].m_Qualities.proc=duplicate?5208:100+i;owner.worn[i+1]=&items[i];portal.table.spells[items[i].m_Qualities.proc]={uint32_t(self?8:0)};}
 owner._sigilProcs=owner.GetSigilProcRate(owner.procSigil);Random::roll=roll;calls.clear();owner.HandleAetheriaProc(target);
 std::cout<<"sigil,"<<mask<<","<<duplicate<<","<<self<<","<<target<<","<<roll<<","<<calls.size();for(auto&c:calls)std::cout<<","<<c.item<<","<<c.target<<","<<c.spell;std::cout<<"\n";
}
// VISUAL
}
