// Synthetic scalar qualities and explicit draws around unchanged GDLE methods.
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <iostream>
#include <iomanip>
#include <map>
#include <string>
using std::min;using std::max;
constexpr bool FALSE=false,TRUE=true;constexpr float F_EPSILON=.0002f;
using DAMAGE_FORM=int;using DAMAGE_TYPE=int;using BODY_PART_ENUM=int;using STypeSkill=int;enum SKILL_ADVANCEMENT_CLASS{UNTRAINED_SKILL_ADVANCEMENT_CLASS=1,TRAINED_SKILL_ADVANCEMENT_CLASS=2,SPECIALIZED_SKILL_ADVANCEMENT_CLASS=3};
enum class DAMAGE_QUADRANT{DQ_UNDEF};constexpr auto DQ_UNDEF=DAMAGE_QUADRANT::DQ_UNDEF;constexpr int BP_UNDEF=0;
enum class COMBAT_USE{COMBAT_USE_MISSILE,COMBAT_USE_SHIELD};
// CONSTANTS
constexpr int DAMAGE_TYPE_INT=9900,LIFE_MAGIC_SKILL=33;
// QUALITY
class CWeenieObject;
// DAMAGE_EVENT
struct Skill{int _sac=0;};
struct Qualities {
 std::map<int,int> ints;std::map<int,uint32_t> attrs;std::map<int,std::pair<int,uint32_t>> skills;
 int GetInt(int key,int fallback){return ints.count(key)?ints[key]:fallback;}
 bool InqInt(int key,int&out){if(!ints.count(key))return false;out=ints[key];return true;}
 bool InqSkill(int key,Skill&out){if(!skills.count(key))return false;out._sac=skills[key].first;return true;}
 bool InqSkill(int key,uint32_t&out,bool=false){out=skills[key].second;return skills.count(key);}
 void InqSkillAdvancementClass(int key,SKILL_ADVANCEMENT_CLASS&out){out=static_cast<SKILL_ADVANCEMENT_CLASS>(skills[key].first);}
 void InqAttribute(int key,uint32_t&out,bool){out=attrs[key];}
 void SetInt(int key,int v){ints[key]=v;}
};
enum ImbuedEffectType {CriticalStrike_ImbuedEffectType=1,CripplingBlow_ImbuedEffectType=2,ArmorRending_ImbuedEffectType=4,SlashRending_ImbuedEffectType=8,PierceRending_ImbuedEffectType=16,BludgeonRending_ImbuedEffectType=32,AcidRending_ImbuedEffectType=64,ColdRending_ImbuedEffectType=128,ElectricRending_ImbuedEffectType=256,FireRending_ImbuedEffectType=512,IgnoreAllArmor_ImbuedEffectType=0x80000000u,IgnoreSomeMagicProjectileDamage_ImbuedEffectType=0x20000000};
constexpr int ThrownShield_CombatStyle=0x80,Atlatl_CombatStyle=0x400,ThrownWeapon_CombatStyle=0x800;
struct ProjectileSpellEx {int _baseIntensity=20,_variance=40;bool AsProjectileSpell(){return true;}};
struct BoostSpellEx:ProjectileSpellEx {int _boost=-20,_boostVariance=-40;int _dt=HEALTH_DAMAGE_TYPE;};
struct Formula {uint32_t level=4;uint32_t GetPowerLevelOfPowerComponent(){return level;}};
struct Spell {uint32_t school=WAR_MAGIC_SKILL;std::string _name="fixture";uint32_t InqSkillForSpell(){return school;}};
struct MetaSpell{ProjectileSpellEx*_spell;};struct SpellEx{MetaSpell _meta_spell;};
struct SpellCastData {Spell*spell;SpellEx*spellEx;uint32_t current_skill;Formula spell_formula;uint32_t wand_id=1;};
struct Config {
#define CS(Name) float GetPkCS##Name##MinSkill(){return 125;}float GetPkCS##Name##MaxSkill(){return 360;}float GetPkCS##Name##BaseChance(){return .05f;}float GetPkCS##Name##MaxChance(){return .25f;}float GetPkCB##Name##MinSkill(){return 125;}float GetPkCB##Name##MaxSkill(){return 360;}float GetPkCB##Name##BaseMult(){return .5f;}
 CS(Magic) CS(Melee) CS(Missile)
 double MissileAttributeAdjust(){return 0;}double voidModifier=1;double VoidDamageReduction(){return voidModifier;}
} config;Config*g_pConfig=&config;
struct Random {static int draw;static float critical,defense,sneak;static double RollDice(double a,double b){return a+(b-a)*.25;}static float GenFloat(float,float){switch(draw++){case 0:return critical;case 1:return defense;default:return sneak;}}};int Random::draw=0;float Random::critical=0,Random::defense=.01f,Random::sneak=.01f;
class CWeenieObject {public:
 bool player=false,peace=false;Qualities m_Qualities;EnchantedQualityDetails resistance;CWeenieObject*missile=nullptr,*shield=nullptr;
 uint32_t imbue=0;double biting=0,crushing=0,elemental=1,angle=180;std::map<int,double> floats;std::map<int,bool> bools;
 CWeenieObject*AsPlayer(){return player?this:nullptr;}bool _IsPlayer(){return player;}bool IsInPeaceMode(){return peace;}
 int InqDamageType(){return COLD_DAMAGE_TYPE;}
 int InqIntQuality(int key,int fallback,bool=true){return m_Qualities.GetInt(key,fallback);}
 bool InqBoolQuality(int key,bool fallback){return bools.count(key)?bools[key]:fallback;}
 double InqFloatQuality(int key,double fallback,bool=true){return floats.count(key)?floats[key]:fallback;}
 void InqSkill(int key,uint32_t&out,bool b){m_Qualities.InqSkill(key,out,b);}
 uint32_t GetRating(int key){return m_Qualities.GetInt(key,0);}uint32_t GetImbueEffects(){return imbue;}
 double GetBitingStrikeFrequency(){return biting;}double GetCrushingBlowMultiplier(){return crushing;}double GetElementalDamageMod(){return elemental;}
 uint32_t GetMagicDefense(){return m_Qualities.skills[MAGIC_DEFENSE_SKILL].second;}
 CWeenieObject*GetWieldedCombat(COMBAT_USE use){return use==COMBAT_USE::COMBAT_USE_MISSILE?missile:shield;}
 bool GetFloatEnchantmentDetails(int,double,EnchantedQualityDetails*out){*out=resistance;out->CalculateEnchantedValue();return true;}
 double GetEffectiveArmorLevel(DamageEventData&,bool){std::abort();}
 double HeadingFrom(CWeenieObject*,bool){return angle;}
 void TakeDamage(DamageEventData&);
};
struct World {CWeenieObject*wand=nullptr;CWeenieObject*FindObject(uint32_t){return wand;}} world;World*g_pWorld=&world;
// RATING
int last_rating;float last_modifier;float GetRatingMod(int rating){last_rating=rating;last_modifier=SourceRating(rating);return last_modifier;}
// METHOD_DECLS
// METHODS
// TAKE_DAMAGE
struct MagicSystem {static uint32_t DeterminePowerLevelOfComponent(uint32_t);};
// FORMULA
struct Projectile {
 CWeenieObject*pSource,*pHit,*wand;SpellCastData m_CachedSpellCastData;bool isLifeProjectile=false;int selfDrainedAmount=30;float selfDrainedDamageRatio=1.2f;double last_before=0;bool last_crit=false,last_defended=false,last_sneak=false;int last_damage=0;
 int InqDamageType(){return COLD_DAMAGE_TYPE;}
 void Hit(bool direct){
 if(direct){auto*m_pWeenie=pSource;auto*target=pHit;auto m_SpellCastData=m_CachedSpellCastData;
// DIRECT
}else{
// PROJECTILE
}}
};
int main(){std::cout<<std::setprecision(17);
 for(int direct:{0,1})for(int sourcePlayer:{0,1})for(int targetPlayer:{0,1})for(uint32_t skill:{124,125,299,300,301,360,400})for(int critical:{0,1})for(int life:{0,1})for(int gear:{0,1,2})for(int defense:{0,1,2,3,4}){
 if(direct&&life)continue;
 CWeenieObject a,d,wand,shield,missile;a.player=sourcePlayer;d.player=targetPlayer;
 a.m_Qualities.ints[DAMAGE_RATING_INT]=20;a.m_Qualities.ints[CRIT_DAMAGE_RATING_INT]=5;d.m_Qualities.ints[DAMAGE_RESIST_RATING_INT]=10;
 a.m_Qualities.skills[SNEAK_ATTACK_SKILL]={3,200};a.m_Qualities.skills[DECEPTION_SKILL]={3,300};d.m_Qualities.skills[PERSONAL_APPRAISAL_SKILL]={2,150};
 d.resistance.rawValue=1;
 if(defense==1){d.m_Qualities.attrs[STRENGTH_ATTRIBUTE]=300;d.m_Qualities.attrs[ENDURANCE_ATTRIBUTE]=200;d.resistance.valueDecreasingMultiplier=.8;d.resistance.valueIncreasingMultiplier=1.5;}
 if(defense==2){d.resistance.rawValue=0;d.m_Qualities.ints[AUGMENTATION_CRITICAL_DEFENSE_INT]=1;}
 if(defense==3){d.shield=&shield;shield.floats[ABSORB_MAGIC_DAMAGE_FLOAT]=.25;d.m_Qualities.skills[SHIELD_SKILL]={3,300};d.m_Qualities.skills[MAGIC_DEFENSE_SKILL]={3,600};d.m_Qualities.ints[AUGMENTATION_RESISTANCE_FAMILY_INT]=1;d.m_Qualities.ints[AUGMENTATION_RESISTANCE_FROST_INT]=2;}
 if(defense==4){d.missile=&missile;missile.imbue=IgnoreSomeMagicProjectileDamage_ImbuedEffectType;d.m_Qualities.skills[MAGIC_DEFENSE_SKILL]={2,400};d.resistance.valueDecreasingMultiplier=.6;}
 wand.m_Qualities.ints[SLAYER_CREATURE_TYPE_INT]=9;d.m_Qualities.ints[CREATURE_TYPE_INT]=9;wand.floats[SLAYER_DAMAGE_BONUS_FLOAT]=2;wand.elemental=1.2;wand.biting=.3;wand.crushing=.2;wand.m_Qualities.ints[DAMAGE_TYPE_INT]=COLD_DAMAGE_TYPE;
 if(gear==2)wand.imbue=CriticalStrike_ImbuedEffectType|CripplingBlow_ImbuedEffectType|ColdRending_ImbuedEffectType;
 BoostSpellEx meta;Spell spell;spell.school=(life||direct)?LIFE_MAGIC_SKILL:WAR_MAGIC_SKILL;SpellEx ex{{&meta}};Projectile projectile{&a,&d,gear?&wand:nullptr,{&spell,&ex,skill,{4}},bool(life)};
 Random::draw=0;Random::critical=critical?0.f:.9f;
 world.wand=gear?&wand:nullptr;wand.floats[ELEMENTAL_DAMAGE_MOD_FLOAT]=1.2;projectile.Hit(direct);
 if(direct)std::cout<<"direct,";
 std::cout<<sourcePlayer<<","<<targetPlayer<<","<<skill<<","<<critical<<","<<life<<","<<gear<<","<<defense<<","<<projectile.last_before<<","<<last_rating<<","<<last_modifier<<","<<projectile.last_crit<<","<<projectile.last_defended<<","<<projectile.last_sneak<<","<<projectile.last_damage<<"\n";
 }

 for(int nether:{0,1})for(int player:{0,1})for(int amount:{1,9,100})for(int reduction:{0,50,100})for(double modifier:{.5,1.}){
 CWeenieObject target;target.player=player;target.resistance.rawValue=1;target.m_Qualities.ints[AUGMENTATION_DAMAGE_REDUCTION_INT]=reduction;target.m_Qualities.ints[DOT_RESIST_RATING_INT]=50;config.voidModifier=modifier;
 DamageEventData event;event.target=&target;event.baseDamage=amount;event.damage_type=nether?NETHER_DAMAGE_TYPE:BASE_DAMAGE_TYPE;event.isDot=nether;target.TakeDamage(event);
 std::cout<<"periodic,"<<nether<<","<<player<<","<<amount<<","<<reduction<<","<<modifier<<","<<event.outputDamageFinal<<"\n";
 }
 for(uint32_t component:{0,1,2,3,4,5,6,7,110,112,192,193,194})std::cout<<"formula,"<<component<<","<<MagicSystem::DeterminePowerLevelOfComponent(component)<<"\n";
}
