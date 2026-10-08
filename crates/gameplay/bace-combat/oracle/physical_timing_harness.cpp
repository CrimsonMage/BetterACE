// Synthetic adapters only. The generator inserts unchanged source methods.
#include <cstdint>
#include <iostream>
#include <iomanip>
#define __thiscall
struct Timer { static double cur_time; }; double Timer::cur_time;
struct InterpretedState { int current_style; } interpreted;
struct CPhysicsObj { static InterpretedState* InqInterpretedMotionState(CPhysicsObj*) {return &interpreted;} } physics;
struct SmartBox { unsigned player_id=1; static SmartBox* smartbox; }; SmartBox box; SmartBox* SmartBox::smartbox=&box;
struct ClientObjMaintSystem {static CPhysicsObj* GetPhysicsObject(unsigned) {return &physics;}};
struct ClientCombatSystem {bool buildInProgress=true; double buildStartTime=10; static long double GetPowerBarLevel(ClientCombatSystem*);};
#define this self
// CLIENT_METHOD
#undef this
struct Qualities {double timestamp=0; void SetFloat(int,double value){timestamp=value;}};
constexpr int ATTACK_TIMESTAMP_FLOAT=1;
struct Weenie { Qualities m_Qualities; bool m_bCancelAttack=false; int done=0,commence=0; Weenie* AsPlayer(){return this;} bool _IsPlayer(){return true;} void NotifyAttackDone(){++done;} void NotifyCommenceAttack(){++commence;} };
struct CAttackEventData {
    double _attack_charge_time=-1; float _attack_power=0, _attack_speed=1; bool dual=false; int begins=0;
    bool IsValidTarget(){return true;} bool ShouldNotifyAttackDone(){return true;}
    float AttackTimeMod(){return dual ? 0.8f : 1.0f;} float AttackSpeedMod(){return dual ? 1.2f : 1.0f;}
    void Begin(){++begins;}
};
#define SafeDelete(p) do {delete (p); (p)=nullptr;} while(0)
struct AttackManager {Weenie* _weenie; CAttackEventData* _attackData=nullptr; CAttackEventData* _queuedAttackData=nullptr; bool repeat=true; bool RepeatAttacks(){return repeat;} void MarkForCleanup(CAttackEventData* p){delete p;} void OnAttackDone(uint32_t);};
// GDLE_METHOD
int main(){
    std::cout<<std::setprecision(17);
    for(int dual: {0,1}) for(double elapsed:{-0.1,0.0,0.2,0.4,0.799,0.8,1.0,1.2}) {
        ClientCombatSystem c; interpreted.current_style=dual ? -2147483578 : 0;Timer::cur_time=10+elapsed;
        std::cout<<"client,"<<dual<<","<<elapsed<<","<<double(ClientCombatSystem::GetPowerBarLevel(&c))<<"\n";
    }
    for(int dual:{0,1}) for(float power:{0.0f,0.25f,0.5f,1.0f}) for(int queued:{0,1}) {
        Weenie w;AttackManager m;m._weenie=&w;m._attackData=new CAttackEventData;m._attackData->_attack_power=power;m._attackData->dual=dual;
        if(queued){m._queuedAttackData=new CAttackEventData;m._queuedAttackData->_attack_power=0.75f;m._queuedAttackData->dual=dual;}
        Timer::cur_time=20;m.OnAttackDone(0);
        std::cout<<"repeat,"<<dual<<","<<power<<","<<queued<<","<<m._attackData->_attack_charge_time<<","<<m._attackData->_attack_power<<","<<w.done<<","<<w.commence<<","<<m._attackData->begins<<"\n";
        delete m._attackData;
    }
}
