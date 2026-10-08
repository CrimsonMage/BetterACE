#include <cmath>
#include <cstdint>
#include <iostream>
#include <iomanip>
#include <vector>
#include <array>
#include <limits>
using BOOL=int;using UINT=unsigned int;
#define TRUE 1
#define FALSE 0
/*EPSILON*/
struct Vector {float x,y,z;/*SCALAR_VECTOR*/};
Vector cross_product(const Vector&,const Vector&);
struct Plane {Vector m_normal;float m_dist;float dot_product(const Vector&);};
struct CSphere {Vector center;float radius;BOOL intersects(CSphere*);};
struct CVertex {Vector origin;};
struct CPolygon {int num_pts;CVertex** vertices;Plane plane;void make_plane();BOOL polygon_hits_sphere_slow_but_sure(CSphere*,Vector*);int hits_sphere(CSphere*);int pos_hits_sphere(CSphere*,Vector*,Vector*,CPolygon**);};
struct BSPNODE {Plane splitting_plane;CSphere sphere;BSPNODE* pos_node=nullptr;BSPNODE* neg_node=nullptr;virtual int sphere_intersects_solid(CSphere*,int);virtual int sphere_intersects_poly(CSphere*,Vector*,CPolygon**,Vector*);virtual ~BSPNODE()=default;};
struct BSPLEAF:BSPNODE {unsigned int num_polys=0;int solid=0;CPolygon** in_polys=nullptr;int sphere_intersects_solid(CSphere*,int)override;int sphere_intersects_poly(CSphere*,Vector*,CPolygon**,Vector*)override;};
// Same-cell adapter only, no world-coordinate conversion is under test.
struct Position {Vector origin;Vector get_offset(const Position& other){return other.origin-origin;}};
struct OBJECTINFO {enum{IS_VIEWER_OI=1};unsigned int state=0;};
struct SPHEREPATH {Position* begin_pos;Position* end_pos;CSphere* local_sphere;};
struct CTransition {OBJECTINFO object_info;SPHEREPATH sphere_path;void calc_num_steps(Vector*,Vector*,unsigned int*);};
/*PINNED_METHODS*/
void vec(Vector v){std::cout<<'['<<v.x<<','<<v.y<<','<<v.z<<']';}
int main(){std::cout<<std::setprecision(9);CVertex vertices[4]={{{-2,-2,0}},{{2,-2,0}},{{2,2,0}},{{-2,2,0}}};CVertex* pointers[4]={&vertices[0],&vertices[1],&vertices[2],&vertices[3]};CPolygon poly;poly.num_pts=4;poly.vertices=pointers;poly.make_plane();CPolygon* polys[1]={&poly};BSPLEAF positive;positive.sphere={{0,0,0},10};positive.num_polys=1;positive.in_polys=polys;BSPLEAF negative;negative.sphere={{0,0,0},10};negative.num_polys=1;negative.in_polys=polys;negative.solid=1;BSPNODE root;root.sphere={{0,0,0},10};root.splitting_plane=poly.plane;root.pos_node=&positive;root.neg_node=&negative;
 std::cout<<"{\"plane\":{\"normal\":";vec(poly.plane.m_normal);std::cout<<",\"distance\":"<<poly.plane.m_dist<<"},\"contacts\":[";bool comma=false;
 std::vector<Vector> centers;for(float x: {-3.0f,-2.5f,-2.0001f,0.0f,2.4998f,2.5f,3.0f})for(float z:{-0.6f,-0.4998f,-0.0001f,0.0f,0.49979f,0.4998f,0.5f,0.6f})centers.push_back({x,0,z});centers.push_back({2.35f,2.35f,0});centers.push_back({2.36f,2.36f,0});centers.push_back({100,0,0});
 for(auto center:centers)for(Vector movement:std::array<Vector,3>{{{0,0,-1},{0,0,1},{1,0,0}}}){CSphere s{center,.5f};Vector point;CPolygon* hit=nullptr;int approaching=root.sphere_intersects_poly(&s,&movement,&hit,&point);if(comma)std::cout<<',';comma=true;std::cout<<"{\"center\":";vec(center);std::cout<<",\"radius\":0.5,\"movement\":";vec(movement);std::cout<<",\"solid_center\":"<<(root.sphere_intersects_solid(&s,1)?"true":"false")<<",\"solid_surface\":"<<(root.sphere_intersects_solid(&s,0)?"true":"false")<<",\"contact\":";if(hit){std::cout<<"{\"point\":";vec(point);std::cout<<",\"approaching\":"<<(approaching?"true":"false")<<'}';}else std::cout<<"null";std::cout<<'}';}
 std::cout<<"],\"steps\":[";comma=false;for(float radius:{.25f,.5f,1.0f})for(Vector end:std::array<Vector,6>{{{0,0,0},{.1f,0,0},{1,0,0},{1.00001f,0,0},{1,2,3},{-3,4,0}}}){Position start{{0,0,0}},finish{end};CSphere sphere{{0,0,0},radius};CTransition t;t.sphere_path={&start,&finish,&sphere};Vector offset,step;unsigned count;t.calc_num_steps(&offset,&step,&count);if(comma)std::cout<<',';comma=true;std::cout<<"{\"radius\":"<<radius<<",\"end\":";vec(end);std::cout<<",\"count\":"<<count<<",\"step\":";vec(step);std::cout<<'}';}BSPLEAF empty;empty.sphere={{-431602080.0f,-431602080.0f,-431602080.0f},-431602080.0f};empty.solid=1;CSphere probe{{0,0,0},.5f};Vector movement{0,0,-1},point;CPolygon* hit=nullptr;
 std::cout<<"],\"empty_leaf\":{\"solid\":"<<(empty.sphere_intersects_solid(&probe,1)?"true":"false")<<",\"approaching\":"<<(empty.sphere_intersects_poly(&probe,&movement,&hit,&point)?"true":"false")<<",\"has_contact\":"<<(hit?"true":"false")<<"}}";}
