// Synthetic dependencies only. Local original client methods are inserted by the
// generator, never redistributed. Native client scalar/little-endian assumptions.
#include <array>
#include <bit>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <iomanip>
#include <vector>
#include <new>
#define __int16 short
using _DWORD=uint32_t; using _WORD=uint16_t; using _BYTE=uint8_t;
#define LODWORD(value) (*reinterpret_cast<uint32_t*>(&(value)))
struct JumpPack;
struct Position;
struct Position_vtbl{int(*Pack)(JumpPack*,void**,unsigned);};
struct Position {Position_vtbl* __vftable;int(*Pack)(Position*,void**,unsigned);uint32_t cell;float origin[3];float rotation[4];};
struct Vector {float x,y,z;};
struct JumpPack {float extent;Vector velocity;Position position;unsigned short instance_timestamp,server_control_timestamp,teleport_timestamp,force_position_ts;};
struct PackObj{static void ALIGN_PTR(void**addr){auto p=reinterpret_cast<uintptr_t>(*addr);*addr=reinterpret_cast<void*>((p+3)&~uintptr_t(3));}};
static int position_size(JumpPack*,void**,unsigned){return 32;}
static int position_pack(Position* p,void**addr,unsigned size){if(size>=32){std::memcpy(*addr,&p->cell,4);auto next=static_cast<char*>(*addr)+4;std::memcpy(next,p->origin,12);std::memcpy(next+12,p->rotation,16);*addr=next+28;}return 32;}
struct OrderHdr_vtbl{};
struct OrderHdr{OrderHdr_vtbl* __vftable;uint32_t stamp_;static unsigned Pack(OrderHdr* self,void**addr,unsigned size){if(size>=4){std::memcpy(*addr,&self->stamp_,4);*addr=static_cast<char*>(*addr)+4;}return 4;}};
static std::vector<unsigned char> sent;
struct Proto_UI{static uint32_t GetNextUICounter(){return 42;}static bool SendToWeenie(char* p,int n){sent.assign(p,p+n);delete[]p;return true;}static void UICounterFailedSend(){}};
// JUMP_METHOD
// NONAUTO_METHOD
static void hex(const unsigned char* bytes,size_t n){for(size_t i=0;i<n;i++)std::cout<<std::hex<<std::setfill('0')<<std::setw(2)<<unsigned(bytes[i]);std::cout<<std::dec;}
int main(){static_assert(std::endian::native==std::endian::little);Position_vtbl vtable{position_size};int index=0;for(float extent:{-1.f,0.f,.75f,1.5f}){JumpPack p{extent,{1.25f,-2.5f,3.75f},{&vtable,position_pack,0x12340100,{1.f,2.f,3.f},{1.f,0.f,0.f,0.f}},0x1122,0x3344,0x5566,0x7788};alignas(4)std::array<unsigned char,128>data{};void*ptr=data.data();int size=pack_jump(&p,&ptr,data.size());if(size!=56||static_cast<unsigned char*>(ptr)!=data.data()+56)return 1;std::cout<<"position,"<<index<<","<<extent<<",";hex(data.data(),size);std::cout<<"\n";pack_nonauto(extent);if(sent.size()!=12)return 2;std::cout<<"nonauto,"<<index<<","<<extent<<",";hex(sent.data(),sent.size());std::cout<<"\n";index++;}}
