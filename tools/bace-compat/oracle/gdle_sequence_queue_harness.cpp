// Synthetic animation lookup, recorder and no-root adapters around original GDLE methods.
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <iostream>
#include <iomanip>
#include <vector>
#include <array>
#include <cstring>
#include <map>
#include <functional>
#define PHATSDK_IS_SERVER 1
using BOOL=int;
#define TRUE 1
#define FALSE 0
#define F_EPSILON (0.0002f)
#include "DLListBase.h"
constexpr uint32_t CM_Action=0x10000000,CM_SubState=0x40000000,CM_Modifier=0x20000000,CM_Style=0x80000000;
struct AFrame{};struct Frame{void combine(Frame*,AFrame*){std::abort();}void subtract1(Frame*,AFrame*){std::abort();}};
struct CAnimHook{static constexpr int BOTH_ANIMHOOK=0;int direction_=0,id=0;CAnimHook *next_hook=nullptr;virtual ~CAnimHook()=default;};struct AnimDoneHook:CAnimHook{AnimDoneHook(){id=-1;}};
struct AnimFrame{CAnimHook *hooks=nullptr;};struct Animation{uint32_t num_frames=0;AFrame *pos_frames=nullptr;AnimFrame *part_frames=nullptr;};
struct AnimData{uint32_t anim_id;int low_frame,high_frame;float framerate;};std::map<uint32_t,Animation*> assets;
struct AnimSequenceNode:DLListData{Animation *anim;float framerate;int low_frame,high_frame;uint32_t tag;AnimSequenceNode(AnimData *d):anim(assets.at(d->anim_id)),framerate(d->framerate),low_frame(d->low_frame),high_frame(d->high_frame),tag(d->anim_id){}bool has_anim(){return anim!=nullptr;}float get_framerate();float get_starting_frame();float get_ending_frame();int32_t get_low_frame();int32_t get_high_frame();AnimSequenceNode *GetNext();AnimSequenceNode *GetPrev();AnimFrame *get_part_frame(int32_t);AFrame *get_pos_frame(int32_t){std::abort();}void multiply_framerate(float);};
struct Recorder{std::vector<int> events;void add_anim_hook(CAnimHook*h){events.push_back(h->id);}};
struct CSequence{DLListBase anim_list;AnimSequenceNode *first_cyclic=nullptr,*curr_anim=nullptr;Recorder *hook_obj=nullptr;double frame_number=0;void update_internal(double,AnimSequenceNode**,double*,Frame*);void advance_to_next_animation(double,AnimSequenceNode**,double*,Frame*);void execute_hooks(AnimFrame*,int);void apricot();void apply_physics(Frame*,double,double){std::abort();}void append_animation(AnimData*);void remove_cyclic_anims();void remove_link_animations(uint32_t);void multiply_cyclic_animation_framerate(float);};
struct AnimNode:DLListData{uint32_t motion=0,num_anims=0;};
struct State{void remove_action_head(){}};struct CallbackWeenie{std::function<void(uint32_t,BOOL)> callback;void OnMotionDone(uint32_t motion,BOOL success){if(callback)callback(motion,success);}};struct PhysicsRecorder{CallbackWeenie *weenie_obj=nullptr;std::vector<uint32_t> completed;void MotionDone(uint32_t id,BOOL success){if(!success)std::abort();completed.push_back(id);}};
struct MotionTableManager{DLListBase pending_animations;State state;PhysicsRecorder *physics_obj;uint32_t animation_counter=0;void CheckForCompletedMotions();void AnimationDone(BOOL);void add_to_queue(uint32_t,uint32_t,CSequence*);void remove_redundant_links(CSequence*);void truncate_animation_list(AnimNode*,CSequence*);};
// WERROR value is an opaque rejection sentinel here; no wire error-number claim.
constexpr int WERROR_ACTIONS_LOCKED=1;
struct Interpreted{std::vector<uint32_t> actions;};struct MInterp{Interpreted interpreted_state;};struct CWeenie{MInterp interp;MInterp *get_minterp(){return &interp;}};
#include "queue_methods.inc"
void clip(CSequence&s,uint32_t id,float rate){AnimData a{id,0,3,rate};s.append_animation(&a);}
void step(CSequence&s,MotionTableManager&m,Recorder&r,double dt){r.events.clear();s.update_internal(dt,&s.curr_anim,&s.frame_number,nullptr);s.apricot();for(int e:r.events)if(e==-1)m.AnimationDone(TRUE);m.CheckForCompletedMotions();}
void output(CSequence&s,PhysicsRecorder&r){uint64_t bits;std::memcpy(&bits,&s.frame_number,8);std::cout<<"{\"animation\":"<<s.curr_anim->tag<<",\"frame_bits\":"<<bits<<",\"rate\":"<<s.curr_anim->framerate<<",\"completed\":[";for(size_t i=0;i<r.completed.size();i++){if(i)std::cout<<",";std::cout<<r.completed[i];}std::cout<<"]}";r.completed.clear();}
int main(){std::cout<<std::setprecision(17)<<"{\"stops\":[";std::array<Animation,5> animations;std::array<std::array<AnimFrame,4>,5> frames;for(int i=0;i<5;i++){animations[i].num_frames=4;animations[i].part_frames=frames[i].data();assets[i+1]=&animations[i];}
 bool comma=false;for(int direction:{1,-1})for(int mode:{0,1})for(int before:{1,5}){if(comma)std::cout<<",";comma=true;CSequence s;Recorder hooks;s.hook_obj=&hooks;MotionTableManager manager;PhysicsRecorder recorder;manager.physics_obj=&recorder;clip(s,1,30.f*direction);clip(s,2,15.f*direction);clip(s,3,30);manager.add_to_queue(0x13000132,2,&s);for(int n=0;n<before;n++)step(s,manager,hooks,double(1.f/30));
 std::cout<<"{\"direction\":"<<direction<<",\"mode\":"<<mode<<",\"before\":"<<before<<",\"initial\":";output(s,recorder);
 if(mode==0){s.multiply_cyclic_animation_framerate(.5f);manager.add_to_queue(0x41000003,0,&s);}else{s.remove_cyclic_anims();clip(s,4,30);clip(s,5,15);manager.add_to_queue(0x41000003,1,&s);}
 std::cout<<",\"stopped\":";output(s,recorder);std::cout<<",\"steps\":[";for(int tick=0;tick<30;tick++){if(tick)std::cout<<",";step(s,manager,hooks,double(1.f/30));output(s,recorder);}std::cout<<"]}";s.anim_list.DestroyContents();manager.pending_animations.DestroyContents();}
 std::cout<<"],\"rates\":[";comma=false;for(float before:{.5f,1.f,2.f})for(float after:{.5f,1.f,2.f}){if(comma)std::cout<<",";comma=true;CSequence s;Recorder hooks;s.hook_obj=&hooks;MotionTableManager manager;PhysicsRecorder recorder;manager.physics_obj=&recorder;clip(s,3,30*before);s.frame_number=2.375;std::cout<<"{\"before\":"<<before<<",\"after\":"<<after;s.multiply_cyclic_animation_framerate(after/before);manager.add_to_queue(0x4000002f,0,&s);std::cout<<",\"changed\":";output(s,recorder);step(s,manager,hooks,double(1.f/30));std::cout<<",\"step\":";output(s,recorder);std::cout<<"}";s.anim_list.DestroyContents();manager.pending_animations.DestroyContents();}
 std::cout<<"],\"coalesced\":{";
 {CSequence s;Recorder hooks;s.hook_obj=&hooks;MotionTableManager manager;PhysicsRecorder recorder;manager.physics_obj=&recorder;clip(s,1,30);clip(s,2,15);clip(s,3,30);manager.add_to_queue(0x41000003,2,&s);step(s,manager,hooks,double(1.f/30));s.remove_cyclic_anims();clip(s,4,30);clip(s,5,15);manager.add_to_queue(0x4000002f,1,&s);s.remove_cyclic_anims();clip(s,1,30);clip(s,3,30);manager.add_to_queue(0x41000003,1,&s);std::cout<<"\"initial\":";output(s,recorder);std::cout<<",\"steps\":[";for(int tick=0;tick<30;tick++){if(tick)std::cout<<",";step(s,manager,hooks,double(1.f/30));output(s,recorder);}std::cout<<"]}";s.anim_list.DestroyContents();manager.pending_animations.DestroyContents();}
 std::cout<<",\"substate_then_action\":{";
 {CSequence s;Recorder hooks;s.hook_obj=&hooks;MotionTableManager manager;PhysicsRecorder recorder;manager.physics_obj=&recorder;CWeenie actor;clip(s,1,30);clip(s,2,15);clip(s,3,30);manager.add_to_queue(0x4000002f,2,&s);step(s,manager,hooks,double(1.f/30));s.remove_cyclic_anims();clip(s,4,30);clip(s,5,15);manager.add_to_queue(0x41000003,1,&s);std::cout<<"\"admitted\":"<<(source_cast_gate(&actor)==0?"true":"false")<<",\"stopped\":";output(s,recorder);
 s.remove_cyclic_anims();clip(s,4,30);clip(s,5,15);manager.add_to_queue(0x13000132,1,&s);actor.interp.interpreted_state.actions.push_back(0x13000132);std::cout<<",\"blocked_after_action\":"<<(source_cast_gate(&actor)!=0?"true":"false")<<",\"admitted_frame\":";output(s,recorder);std::cout<<",\"steps\":[";for(int tick=0;tick<30;tick++){if(tick)std::cout<<",";step(s,manager,hooks,double(1.f/30));output(s,recorder);}std::cout<<"]}";s.anim_list.DestroyContents();manager.pending_animations.DestroyContents();}
 std::cout<<",\"reentrant\":[";
 for(int trigger:{0,1}){if(trigger)std::cout<<",";CSequence s;Recorder hooks;s.hook_obj=&hooks;MotionTableManager manager;PhysicsRecorder recorder;CallbackWeenie weenie;recorder.weenie_obj=&weenie;manager.physics_obj=&recorder;
  if(trigger==0){AnimData link{1,0,0,30};s.append_animation(&link);clip(s,2,30);manager.add_to_queue(0x13000132,1,&s);weenie.callback=[&](uint32_t motion,BOOL){if(motion==0x13000132){s.remove_cyclic_anims();clip(s,3,0);manager.add_to_queue(0x4000002f,0,&s);}else if(motion==0x4000002f){s.remove_cyclic_anims();clip(s,4,0);manager.add_to_queue(0x40000030,0,&s);}};step(s,manager,hooks,double(1.f/30));}
  else {clip(s,3,30);s.frame_number=2.375;manager.add_to_queue(0x4000002f,0,&s);int remaining=2;weenie.callback=[&](uint32_t motion,BOOL){if(remaining-->0)manager.add_to_queue(motion,0,&s);};manager.CheckForCompletedMotions();}
  std::cout<<"{\"trigger\":"<<trigger<<",\"physics_updates\":"<<(trigger==0?1:0)<<",\"result\":";output(s,recorder);std::cout<<"}";s.anim_list.DestroyContents();manager.pending_animations.DestroyContents();}
 std::cout<<"]}\n";
}
