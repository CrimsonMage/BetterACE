#include <cstdint>
#include <cfloat>
#include <cmath>
#include <list>
#include <iostream>
#include <cstring>
using BOOL=int;using WORD=uint16_t;constexpr bool TRUE=true;
// ERRORS
// CONSTANTS
constexpr uint32_t Motion_NonCombat=0x8000003d;
constexpr int Resistable_SpellIndex=1,PKSensitive_SpellIndex=2,FastCast_SpellIndex=4,NEXT_SPELLCAST_TIMESTAMP_FLOAT=1,PS_Fizzle=1,LTT_MAGIC=1,LTT_ERROR=2;
namespace Timer {double cur_time=10;}
namespace MovementTypes {enum {RawCommand=1,TurnToObject=2};}
struct MovementParameters{float speed=0;WORD action_stamp=0;int modify_interpreted_state=0;};
struct MovementStruct{int type=0;uint32_t motion=0;MovementParameters*params=nullptr;};
struct Position {double value=0;double distance(const Position&o)const{return std::abs(value-o.value);}};
struct Quality {double next=0;double GetFloat(int,double)const{return next;}void SetFloat(int,double value){next=value;}};
struct Spell {int _bitfield=0;};
struct Formula {int GetPowerLevelOfPowerComponent(){return 7;}};
struct SpellCastData {uint32_t target_id=2,caster_id=1,source_id=1,spell_id=1,wand_id=0;bool uses_mana=true;int power_level_of_power_component=0;Formula spell_formula;Spell*spell=nullptr;double cast_timeout=FLT_MAX,next_update=Timer::cur_time;Position initial_cast_position;};
struct Interpreted {int turn_command=0;uint32_t current_style=0x80000049;};struct Interpreter {Interpreted interpreted_state;};struct MoveTo {int movement_type=0;};struct MovementManager {MoveTo move;MoveTo*moveto_manager=&move;};
class CSpellcastingManager;
class CWeenieObject {
public:CSpellcastingManager*m_SpellcastingManager=nullptr;MovementManager manager;MovementManager*movement_manager=&manager;Interpreter interpreter;Quality m_Qualities;Position m_Position;bool last_move_was_autonomous=true;WORD m_wAnimSequence=0;uint16_t _server_control_timestamp=0;int turns=0,motions=0,releases=0,stops=0,fizzles=0,mana=100,error=-1;bool dead=false,player=true;float last_intensity=0;
 bool IsDead(){return dead;}bool IsInPeaceMode();CWeenieObject*AsPlayer(){return player?this:nullptr;}CWeenieObject*GetWorldTopLevelOwner(){return nullptr;}uint32_t GetID(){return 2;}uint32_t GetTopLevelID(){return 2;}uint32_t GetWieldedCasterID(){return 0;}Interpreter*get_minterp(){return &interpreter;}
 void DoForcedStopCompletely(){stops++;}void NotifyUseDone(int e){error=e;}void cancel_moveto(){manager.move.movement_type=0;}
 void TurnToObject(uint32_t,MovementParameters*){turns++;manager.move.movement_type=MovementTypes::TurnToObject;}void StopCompletely(int){stops++;}
 int PerformMovement(MovementStruct){motions++;return 0;}void Animation_Update(){}void EmitEffect(int,float value){fizzles++;last_intensity=value;}int GetMaxMana(){return 100;}int GetMana(){return mana;}void SetMana(int value){mana=value;}int AdjustMana(int amount);void SendText(const char*,int){}
};
struct World {CWeenieObject object;CWeenieObject*FindObject(uint32_t){return &object;}} world;World*g_pWorld=&world;
class CSpellcastingManager {
public:struct SpellCastingMotion{uint32_t motion=123;float speed=2;bool requiresHeading=true;float min_time=1;};
 CWeenieObject*m_pWeenie;SpellCastData m_SpellCastData;std::list<SpellCastingMotion>m_PendingMotions;bool m_bCasting=true,m_bTurningToObject=false,m_bTurned=false;double m_fNextCastTime=0;float heading=0;Spell spell;
 CSpellcastingManager(CWeenieObject*p):m_pWeenie(p){p->m_SpellcastingManager=this;m_SpellCastData.spell=&spell;m_SpellCastData.cast_timeout=1000;}
 float HeadingToTarget(){return heading;}int LaunchSpellEffect(bool fizzled){if(!fizzled)m_pWeenie->releases++;return 0;}
 bool ResolveSpellBeingCasted(){m_SpellCastData.spell=&spell;return true;}int CastSpellInstant(uint32_t,uint32_t);
 void EndCast(int);bool MotionRequiresHeading();void BeginNextMotion();void Update();void HandleMotionDone(uint32_t,BOOL);
};
// METHODS
int main(){
 for(float heading:{0.0f,45.0f,45.01f,90.0f})for(int pending:{0,1}){Timer::cur_time=10;CWeenieObject actor;CSpellcastingManager c(&actor);c.heading=heading;if(pending)c.m_PendingMotions.push_back({});c.BeginNextMotion();std::cout<<"begin,"<<heading<<","<<pending<<","<<actor.turns<<","<<actor.motions<<","<<actor.releases<<","<<c.m_fNextCastTime<<"\n";}
 for(float heading:{0.0f,45.0f,45.01f,90.0f})for(int active:{0,1})for(int manual:{0,1}){Timer::cur_time=10;CWeenieObject actor;CSpellcastingManager c(&actor);c.heading=heading;c.m_PendingMotions.push_back({});c.m_bTurningToObject=true;actor.manager.move.movement_type=active?MovementTypes::TurnToObject:0;actor.interpreter.interpreted_state.turn_command=manual;Timer::cur_time=11;c.Update();std::cout<<"update,"<<heading<<","<<active<<","<<manual<<","<<actor.turns<<","<<actor.motions<<","<<c.m_bCasting<<"\n";}
 for(double distance:{5.999,6.0,6.001}){Timer::cur_time=10;CWeenieObject actor;CSpellcastingManager c(&actor);c.m_bTurned=true;actor.m_Position.value=distance;Timer::cur_time=11;c.Update();std::cout<<"distance,"<<distance<<","<<c.m_bCasting<<","<<actor.mana<<","<<actor.fizzles<<"\n";}
 for(bool player:{false,true})for(uint32_t style:{0x8000003du,0x8000003eu,0x80000048u,0x80000049u})for(int mana:{0,3,5,100}){Timer::cur_time=10;CWeenieObject actor;actor.player=player;actor.mana=mana;actor.interpreter.interpreted_state.current_style=style;CSpellcastingManager c(&actor);Timer::cur_time=11;c.Update();uint32_t intensity;std::memcpy(&intensity,&actor.last_intensity,4);std::cout<<"mode,"<<player<<","<<style<<","<<mana<<","<<c.m_bCasting<<","<<actor.mana<<","<<actor.fizzles<<","<<actor.releases<<","<<actor.error<<","<<intensity<<"\n";}

 for(bool peace:{false,true}){Timer::cur_time=10;CWeenieObject actor;actor.interpreter.interpreted_state.current_style=peace?Motion_NonCombat:0x80000049;CSpellcastingManager c(&actor);c.m_PendingMotions.push_back({});Timer::cur_time=11;c.HandleMotionDone(123,TRUE);c.Update();std::cout<<"callback_mode,"<<peace<<","<<actor.releases<<","<<actor.fizzles<<","<<actor.mana<<","<<c.m_bCasting<<"\n";}

 for(bool dead:{false,true})for(bool busy:{false,true})for(bool player:{false,true})for(int mana:{0,100}){
  Timer::cur_time=10;CWeenieObject actor;actor.dead=dead;actor.player=player;actor.mana=mana;CSpellcastingManager c(&actor);c.m_bCasting=busy;int error=c.CastSpellInstant(3,7);
  std::cout<<"instant,"<<dead<<","<<busy<<","<<player<<","<<mana<<","<<actor.releases<<","<<actor.mana<<","<<c.m_bCasting<<","<<error<<"\n";
 }

}
