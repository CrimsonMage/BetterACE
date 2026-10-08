#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <iostream>
#include <iomanip>
#include <vector>
#include <array>
#include <cstring>
#include <map>
using BOOL=int;
#define TRUE 1
#define FALSE 0
#define F_EPSILON (0.0002f)
#include "DLListBase.h"
struct AFrame {};
struct Frame {void combine(Frame*,AFrame*){std::abort();}void subtract1(Frame*,AFrame*){std::abort();}};
struct CAnimHook {static constexpr int BOTH_ANIMHOOK=0;int direction_=0;CAnimHook *next_hook=nullptr;int id=0;virtual ~CAnimHook()=default;};
struct AnimDoneHook:CAnimHook {AnimDoneHook(){id=-1;}};
struct AnimFrame {CAnimHook *hooks=nullptr;};
struct Animation {uint32_t num_frames=0;AFrame *pos_frames=nullptr;AnimFrame *part_frames=nullptr;};
struct AnimSequenceNode:DLListData {Animation *anim=nullptr;float framerate=0;int32_t low_frame=0,high_frame=0;int tag=0;float get_framerate();float get_starting_frame();float get_ending_frame();int32_t get_low_frame();int32_t get_high_frame();AnimSequenceNode *GetNext();AnimSequenceNode *GetPrev();AnimFrame *get_part_frame(int32_t);AFrame *get_pos_frame(int32_t){std::abort();}};
struct Recorder {std::vector<int> events;void add_anim_hook(CAnimHook *hook){events.push_back(hook->id);}};
struct CSequence {std::vector<std::pair<uint32_t,float>> prepared;bool has_anims(){return true;}float rate_multiplier=1;void multiply_cyclic_animation_framerate(float value){rate_multiplier*=value;}void clear_physics(){}void remove_cyclic_anims(){}DLListBase anim_list;AnimSequenceNode *first_cyclic=nullptr,*curr_anim=nullptr;Recorder *hook_obj=nullptr;void update_internal(double,AnimSequenceNode**,double*,Frame*);void advance_to_next_animation(double,AnimSequenceNode**,double*,Frame*);void execute_hooks(AnimFrame*,int);void apricot();void apply_physics(Frame*,double,double){std::abort();}};
struct MotionData {uint32_t id,num_anims;uint8_t bitfield=0;};
template<class T>struct LongHash {std::map<uint32_t,T*> values;T* lookup(uint32_t key){auto i=values.find(key);return i==values.end()?nullptr:i->second;}};
template<class T>struct Lookup {std::map<uint32_t,T> values;BOOL lookup(uint32_t key,T *out){auto i=values.find(key);if(i==values.end())return FALSE;*out=i->second;return TRUE;}};
struct MotionState {uint32_t style=0,substate=0;float substate_mod=1;void remove_action_head(){}void add_action(uint32_t,float){}void clear_modifiers(){}void add_modifier_no_check(uint32_t,float){}};
constexpr uint32_t CM_Action=0x10000000,CM_SubState=0x40000000;
struct CMotionTable {uint32_t default_style=0x80000049;BOOL is_allowed(uint32_t,MotionData*,MotionState*);Lookup<uint32_t> style_defaults;Lookup<LongHash<MotionData>*> links;LongHash<MotionData> cycles;MotionData *get_link(uint32_t,uint32_t,float,uint32_t,float);BOOL action(uint32_t,MotionState*,CSequence*,float,uint32_t*);void re_modify(CSequence*,MotionState*){}};
void add_motion(CSequence *sequence,MotionData *data,float speed){if(data)sequence->prepared.push_back({data->id,speed});}
void subtract_motion(CSequence*,MotionData*,float){}void combine_motion(CSequence*,MotionData*,float){}
struct AnimNode:DLListData{uint32_t motion=0,num_anims=0;};
struct PhysicsRecorder{std::vector<uint32_t> completed;void MotionDone(uint32_t id,BOOL success){if(!success)std::abort();completed.push_back(id);}};
struct MotionTableManager{DLListBase pending_animations;MotionState state;PhysicsRecorder *physics_obj;void CheckForCompletedMotions();};
#include "methods.inc"
int main(){std::cout<<std::setprecision(17)<<"{\"execution\":[";bool first=true;
 for(float dt:{1.0f/30.0f,.01f,.1f,.2f})for(int direction:{1,-1})for(float speed:{.5f,1.0f,2.0f}){
  if(!first)std::cout<<",";first=false;CSequence seq;Recorder recorder;seq.hook_obj=&recorder;
  std::array<Animation,3> animations;std::array<std::array<AnimFrame,4>,3> frames;std::array<std::array<std::array<CAnimHook,4>,4>,3> hooks;
  for(int segment=0;segment<3;segment++){animations[segment].num_frames=4;animations[segment].part_frames=frames[segment].data();for(int frame=0;frame<4;frame++){for(int h=0;h<4;h++){auto &hook=hooks[segment][frame][h];hook.direction_=h==0?0:h==1?1:h==2?-1:-2;hook.id=(segment+1)*100+frame*10+h;hook.next_hook=h==3?nullptr:&hooks[segment][frame][h+1];}frames[segment][frame].hooks=&hooks[segment][frame][0];}
   auto *node=new AnimSequenceNode();node->anim=&animations[segment];node->low_frame=segment==0?1:0;node->high_frame=3;node->framerate=(segment==1?15.0f:30.0f)*speed*direction;node->tag=segment;seq.anim_list.InsertAfter(node,seq.anim_list.tail_);if(segment==2)seq.first_cyclic=node;
  }
  seq.curr_anim=static_cast<AnimSequenceNode*>(seq.anim_list.head_);double frame=seq.curr_anim->get_starting_frame();uint32_t dt_bits;std::memcpy(&dt_bits,&dt,4);std::cout<<"{\"dt\":"<<double(dt)<<",\"dt_bits\":"<<dt_bits<<",\"speed\":"<<speed<<",\"direction\":"<<direction<<",\"steps\":[";
  for(int tick=0;tick<100;tick++){recorder.events.clear();seq.update_internal(double(dt),&seq.curr_anim,&frame,nullptr);seq.apricot();uint64_t frame_bits;std::memcpy(&frame_bits,&frame,8);if(tick)std::cout<<",";std::cout<<"{\"frame\":"<<frame<<",\"frame_bits\":"<<frame_bits<<",\"segment\":"<<seq.curr_anim->tag<<",\"events\":[";for(size_t i=0;i<recorder.events.size();i++){if(i)std::cout<<",";std::cout<<recorder.events[i];}std::cout<<"]}";}
  std::cout<<"]}";seq.anim_list.DestroyContents();
 }
 std::cout<<"],\"chains\":[";
 for(int scenario=0;scenario<27;scenario++) {
  if(scenario)std::cout<<",";CMotionTable table;MotionState state;state.style=0x80000049;state.substate=0x45000005;state.substate_mod=scenario==6||scenario==8?-.5f:1.0f;uint32_t requested=scenario==10?0x04000005:0x13000132;float speed=scenario==7?-2.0f:2.0f;
  if(scenario==9)state.style=0;
  if(scenario>=12)requested=0x4000002b;
  if(scenario>=24){state.substate=requested;state.substate_mod=scenario==24?.5f:scenario==25?1.f:2.f;}
  constexpr uint32_t style=0x80000049,current=0x45000005,ready=0x41000003,action=0x13000132;
  table.style_defaults.values[style]=ready;MotionData direct{11,2},to_default{12,1},back{13,3},cycle{14,scenario==11?2u:1u};std::map<uint32_t,MotionData*> data{{11,&direct},{12,&to_default},{13,&back},{14,&cycle}};std::array<LongHash<MotionData>,4> buckets;
  uint32_t current_key=(style<<16)|(current&0xffffff),ready_key=(style<<16)|(ready&0xffffff),action_key=(style<<16)|(action&0xffffff);
  if(scenario!=4)table.cycles.values[current_key]=&cycle;
  if(scenario==0||scenario==4||scenario==9||scenario==10||scenario==11){table.links.values[current_key]=&buckets[0];buckets[0].values[action]=&direct;}
  if(scenario==1){table.links.values[style<<16]=&buckets[0];buckets[0].values[action]=&direct;}
  if(scenario==2||scenario==3||scenario==5||scenario==8){table.links.values[current_key]=&buckets[0];buckets[0].values[ready]=&to_default;table.links.values[ready_key]=&buckets[1];if(scenario!=5)buckets[1].values[action]=&direct;if(scenario==2)buckets[1].values[current]=&back;}
  if(scenario==6||scenario==7){table.links.values[action_key]=&buckets[0];buckets[0].values[current]=&direct;}
  if(scenario>=12){
   table.cycles.values.clear();table.links.values.clear();for(auto &b:buckets)b.values.clear();
   uint32_t targetkey=(style<<16)|(requested&0xffffff);
   if(scenario!=20)table.cycles.values[targetkey]=&cycle;
   if(scenario==12||scenario==19||scenario==21){table.links.values[current_key]=&buckets[0];buckets[0].values[requested]=&direct;}
   if(scenario==13||scenario==14){table.links.values[current_key]=&buckets[0];buckets[0].values[ready]=&to_default;table.links.values[ready_key]=&buckets[1];if(scenario==13)buckets[1].values[requested]=&direct;}
   if(scenario==15){table.links.values[style<<16]=&buckets[0];buckets[0].values[requested]=&direct;}
   if(scenario==16){state.style=0x8000003c;table.style_defaults.values[state.style]=ready;}
   if(scenario==17||scenario==18){state.substate_mod=-.5f;table.links.values[current_key]=&buckets[0];buckets[0].values[ready]=&to_default;if(scenario==17){table.links.values[ready_key]=&buckets[1];buckets[1].values[requested]=&direct;}}
   if(scenario==19||scenario==21){cycle.bitfield=2;if(scenario==21)state.substate=ready;}
   if(scenario==22)cycle.num_anims=3;
   if(scenario==23){state.substate_mod=-.5f;speed=-2;table.links.values[targetkey]=&buckets[0];buckets[0].values[current]=&direct;}
  }
  uint32_t input_style=state.style,input_current=state.substate;float input_speed=state.substate_mod;
  CSequence sequence;uint32_t completion=0;bool ok=table.action(requested,&state,&sequence,speed,&completion);
  std::cout<<"{\"scenario\":"<<scenario<<",\"style\":"<<input_style<<",\"current\":"<<input_current<<",\"current_speed\":"<<input_speed<<",\"action\":"<<requested<<",\"action_speed\":"<<speed<<",\"default\":"<<ready<<",\"data\":[";bool comma=false;for(auto p:data){if(comma)std::cout<<",";comma=true;std::cout<<"["<<p.first<<","<<p.second->num_anims<<","<<unsigned(p.second->bitfield)<<"]";}std::cout<<"],\"cycles\":[";comma=false;for(auto p:table.cycles.values){if(comma)std::cout<<",";comma=true;std::cout<<"["<<p.first<<","<<p.second->id<<"]";}std::cout<<"],\"links\":[";comma=false;for(auto p:table.links.values)for(auto q:p.second->values){if(comma)std::cout<<",";comma=true;std::cout<<"["<<p.first<<","<<q.first<<","<<q.second->id<<"]";}std::cout<<"],\"accepted\":"<<(ok?"true":"false")<<",\"completion\":"<<completion<<",\"parts\":[";comma=false;for(auto p:sequence.prepared){if(comma)std::cout<<",";comma=true;std::cout<<"["<<p.first<<","<<p.second<<"]";}std::cout<<"]}";
 }
 std::cout<<"],\"zero_completion\":[";
 for(uint32_t count:{0u,1u}){if(count)std::cout<<",";MotionTableManager manager;PhysicsRecorder recorder;manager.physics_obj=&recorder;auto *node=new AnimNode;node->motion=0x4000002b;node->num_anims=count;manager.pending_animations.InsertAfter(node,nullptr);manager.CheckForCompletedMotions();std::cout<<"{\"count\":"<<count<<",\"completed\":"<<recorder.completed.size()<<"}";manager.pending_animations.DestroyContents();}
 std::cout<<"],\"static_cycle\":{";
 {CSequence seq;Recorder recorder;seq.hook_obj=&recorder;Animation animation;animation.num_frames=1;AnimFrame framepart;animation.part_frames=&framepart;auto *node=new AnimSequenceNode;node->anim=&animation;node->framerate=0;node->low_frame=node->high_frame=0;seq.anim_list.InsertAfter(node,nullptr);seq.first_cyclic=seq.curr_anim=node;double frame=0;seq.update_internal(double(.2f),&seq.curr_anim,&frame,nullptr);std::cout<<"\"frame\":"<<frame<<",\"events\":"<<recorder.events.size();seq.anim_list.DestroyContents();}
 std::cout<<"}}\n";
}
