#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <iomanip>
#include <array>
using BOOL=int;
#define TRUE 1
#define FALSE 0
/*CONSTANTS*/
struct Vector {float x,y,z;/*VECTOR*/};
struct Frame {Vector m_origin;void grotate(Vector&v){if(v.x!=0||v.y!=0||v.z!=0)std::abort();}};
constexpr uint32_t ON_WALKABLE_TS=2,ACTIVE_TS=128;
struct CPhysicsObj {Vector m_velocityVector{0,0,0},m_Acceleration{0,0,0},m_Omega{0,0,0};void *movement_manager=nullptr;uint32_t transient_state=0;int jumped_this_frame=0;void calc_friction(float,float){if(transient_state&ON_WALKABLE_TS)std::abort();}void UpdatePhysicsInternal(float,Frame&);void set_test_velocity(const Vector&);};
struct Position {uint32_t objcell_id;Frame frame;Vector get_offset(const Position&) const;};
/*METHODS*/
void vector(Vector v){std::cout<<'['<<v.x<<','<<v.y<<','<<v.z<<']';}
void bits(Vector v){uint32_t b[3];std::memcpy(b,&v,12);std::cout<<'['<<b[0]<<','<<b[1]<<','<<b[2]<<']';}
int main(){std::cout<<std::setprecision(9)<<"{\"flight\":[";bool comma=false;
 for(Vector velocity:std::array<Vector,9>{{{0,0,0},{.0001f,0,.0001f},{.0002f,0,0},{.1f,.1f,0},{.25f,0,0},{.251f,0,0},{20,0,10},{45,30,0},{120,-80,400}}})for(float gravity:{0.f,-9.8f})for(float dt:{1.f/30.f,.1f,.2f}) {
 if(comma)std::cout<<',';comma=true;CPhysicsObj p;p.m_Acceleration={0,0,gravity};p.set_test_velocity(velocity);Frame frame{{1,-2,3}};std::cout<<"{\"velocity\":";vector(velocity);std::cout<<",\"gravity\":"<<gravity<<",\"dt\":"<<dt<<",\"initial_velocity\":";bits(p.m_velocityVector);std::cout<<",\"steps\":[";
 for(int step=0;step<60;step++){if(step)std::cout<<',';p.UpdatePhysicsInternal(dt,frame);std::cout<<"{\"position\":";bits(frame.m_origin);std::cout<<",\"velocity\":";bits(p.m_velocityVector);std::cout<<'}';}std::cout<<"]}";
 }std::cout<<"],\"frames\":[";comma=false;
 for(float square:{.1f,24.f,1000.f})for(int side:{2,4,8,16})for(auto cells:std::array<std::array<uint32_t,2>,4>{{{0xa2600001u,0xa2600100u},{0xa2600001u,0xa3600001u},{0xa2600001u,0xa25f0001u},{0x01020001u,0xc8800100u}}}) {
 LandDefs::square_length=square;LandDefs::lblock_shift=int(std::log2(side));if(comma)std::cout<<',';comma=true;Position from{cells[0],{{190,17,4}}},to{cells[1],{{3,19,7}}};std::cout<<"{\"square\":"<<square<<",\"side\":"<<side<<",\"from\":"<<cells[0]<<",\"to\":"<<cells[1]<<",\"offset\":";bits(LandDefs::get_block_offset(cells[0],cells[1]));std::cout<<",\"delta\":";bits(from.get_offset(to));std::cout<<'}';
 }std::cout<<"]}\n";
}
