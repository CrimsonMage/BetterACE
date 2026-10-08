using System;
using System.IO;
using System.Linq;
using System.Collections.Generic;
using System.Text.Json;
using ACE.DatLoader.FileTypes;
using ACE.DatLoader;
using ACE.Server.Physics.Common;
using ACE.Server.Physics.Animation;
namespace ACE.Common {}
namespace ACE.Entity {public class Position {public System.Numerics.Vector3 Pos; public System.Numerics.Quaternion Rotation;}}
namespace ACE.Entity {public class Frame {public System.Numerics.Vector3 Origin;public System.Numerics.Quaternion Orientation;}}
namespace ACE.DatLoader.Entity {public class AnimationFrame {}}
namespace ACE.Server.Physics {public class Sphere {public Sphere(System.Numerics.Vector3 v,float r){}}public class PhysicsObj {public void add_anim_hook(object value){throw new Exception("unused hook owner");}}}
namespace ACE.Server.Physics.Hooks {public class AnimationHook {public static object AnimDoneHook=new();}}
namespace ACE.Server.Physics.Common {public static class DBObj {public static object GetAnimation(uint id)=>throw new Exception("unused DAT loading");}public static class Vec {public static bool NormalizeCheckSmall(ref System.Numerics.Vector3 value)=>throw new Exception("unused");}}
namespace ACE.Server.Physics.Extensions {public static class Extensions {public static bool IsValid(this System.Numerics.Vector3 v)=>throw new Exception("unused");public static bool IsValid(this System.Numerics.Quaternion v)=>throw new Exception("unused");public static float ToDegrees(this float v)=>throw new Exception("unused");public static float ToRadians(this float v)=>throw new Exception("unused");}}
namespace ACE.Server.Physics.Animation {public class Animation {public uint NumFrames;public List<AFrame> PosFrames=new();public List<ACE.DatLoader.Entity.AnimationFrame> PartFrames=new();public Animation(){}public Animation(object value){throw new Exception("unused DAT conversion");}}}
class Program {
 static object Cmt() {
  using var stream=new MemoryStream();using var w=new BinaryWriter(stream);
  w.Write(0x30000000u);w.Write(3u);
  foreach(var row in new uint[][]{new uint[]{0x8000003d,1,2,0,0x10000001},new uint[]{0x8000003d,1,2,25,0x10000002},new uint[]{0xffffffff,2,0xffffffff,0xffffffff,0x80000000}})foreach(var value in row)w.Write(value);
  var bytes=stream.ToArray();var table=new CombatManeuverTable();using var r=new BinaryReader(new MemoryStream(bytes));table.Unpack(r);
  return new{bytes=Convert.ToHexString(bytes),id=table.Id,rows=table.CMT.Select(m=>new uint[]{(uint)m.Style,(uint)m.AttackHeight,(uint)m.AttackType,m.MinSkillLevel,(uint)m.Motion})};
 }
 static void Align(BinaryWriter w){while(w.BaseStream.Position%4!=0)w.Write((byte)0);}
 static void Text(BinaryWriter w,string text){var bytes=System.Text.Encoding.UTF8.GetBytes(text);w.Write((ushort)bytes.Length);w.Write(bytes);Align(w);}
 static void Frame(BinaryWriter w,float n){foreach(float f in new[]{n,-n,n+.25f,1f,0f,0f,0f})w.Write(f);}
 static object F(ACE.DatLoader.Entity.Frame f)=>new{origin=new[]{f.Origin.X,f.Origin.Y,f.Origin.Z},rotation=new[]{f.Orientation.W,f.Orientation.X,f.Orientation.Y,f.Orientation.Z}};
 static object P(ACE.DatLoader.Entity.Position p)=>new{cell=p.ObjCellID,frame=F(p.Frame)};
 static object Contracts(){using var stream=new MemoryStream();using var w=new BinaryWriter(stream);w.Write(0x0e00001du);w.Write((ushort)2);w.Write((ushort)7);for(uint i=1;i<=2;i++){w.Write(i);w.Write(3u);w.Write(i);foreach(var name in new[]{"café","description","progress","start","end","stamp","started","finished","progressflag","timer","repeat"})Text(w,name+i);for(uint j=0;j<3;j++){w.Write(0x12340001+j);Frame(w,i+j);}}var bytes=stream.ToArray();var table=new ContractTable();using var r=new BinaryReader(new MemoryStream(bytes));table.Unpack(r);return new{bytes=Convert.ToHexString(bytes),rows=table.Contracts.ToDictionary(p=>p.Key,p=>new{version=p.Value.Version,id=p.Value.ContractId,name=p.Value.ContractName,description=p.Value.Description,progress_description=p.Value.DescriptionProgress,start_npc=p.Value.NameNPCStart,end_npc=p.Value.NameNPCEnd,stamped_quest=p.Value.QuestflagStamped,started_quest=p.Value.QuestflagStarted,finished_quest=p.Value.QuestflagFinished,progress_quest=p.Value.QuestflagProgress,timer_quest=p.Value.QuestflagTimer,repeat_time_quest=p.Value.QuestflagRepeatTime,start_location=P(p.Value.LocationNPCStart),end_location=P(p.Value.LocationNPCEnd),quest_location=P(p.Value.LocationQuestArea)})};}
 static object LandInfo(){using var stream=new MemoryStream();using var w=new BinaryWriter(stream);w.Write(0x1234fffeu);w.Write(2u);w.Write(1u);w.Write(0x01000001u);Frame(w,1);w.Write((ushort)1);w.Write((ushort)1);w.Write(0x02000002u);Frame(w,2);w.Write(7u);w.Write(1u);w.Write((ushort)3);w.Write((ushort)0x100);w.Write((ushort)1);w.Write((ushort)3);foreach(ushort x in new ushort[]{0x100,0x101,0x102})w.Write(x);Align(w);w.Write((ushort)1);w.Write((ushort)8);w.Write(0x12340001u);w.Write(0x80000123u);var bytes=stream.ToArray();var info=new LandblockInfo();using var r=new BinaryReader(new MemoryStream(bytes));info.Unpack(r);return new{bytes=Convert.ToHexString(bytes),id=info.Id,cells=info.NumCells,pack_mask=info.PackMask,objects=info.Objects.Select(o=>new{id=o.Id,frame=F(o.Frame)}),buildings=info.Buildings.Select(b=>new{model=b.ModelId,frame=F(b.Frame),leaves=b.NumLeaves,portals=b.Portals.Select(p=>new{flags=(ushort)p.Flags,other_cell=p.OtherCellId,other_portal=p.OtherPortalId,visible_cells=p.StabList})}),restrictions=info.RestrictionTables};}
 static object Land(){using var stream=new MemoryStream();using var w=new BinaryWriter(stream);w.Write(0x13000000u);w.Write(1u);w.Write(2u);Text(w,"Dereth");w.Write(2048);w.Write(2048);w.Write(24f);w.Write(8);w.Write(1);w.Write(100f);w.Write(2000f);w.Write(4f);for(int i=0;i<256;i++)w.Write(i*2.75f);var bytes=stream.ToArray();using var r=new BinaryReader(new MemoryStream(bytes));r.ReadUInt32();r.ReadUInt32();r.ReadUInt32();r.ReadPString();r.AlignBoundary();var land=new ACE.DatLoader.Entity.LandDefs();land.Unpack(r);return new{bytes=Convert.ToHexString(bytes),length=land.NumBlockLength,width=land.NumBlockWidth,square=land.SquareLength,block=land.LBlockLength,vertices=land.VertexPerCell,max_height=land.MaxObjHeight,sky_height=land.SkyHeight,road_width=land.RoadWidth,heights=land.LandHeightTable};}
 static object RootMotions() {
  var cases=new List<object>();foreach(float dt in new[]{1f/30f,.01f,.1f})foreach(float speed in new[]{0f,.25f,1f,2.65f,-.65f,-1.5f}) {
   var oracle=new RootOracle {Velocity=new System.Numerics.Vector3(1,2,0),Omega=new System.Numerics.Vector3(0,0,.3f)};
   for(int s=0;s<2;s++){var animation=new ACE.Server.Physics.Animation.Animation{NumFrames=(uint)(4-s)};for(int i=0;i<4-s;i++){animation.PosFrames.Add(new AFrame(new System.Numerics.Vector3(i*.001f,.13f+i*.01f,0),System.Numerics.Quaternion.CreateFromAxisAngle(System.Numerics.Vector3.UnitZ,i*.001f)));animation.PartFrames.Add(new ACE.DatLoader.Entity.AnimationFrame());}oracle.AnimList.AddLast(new AnimSequenceNode{Anim=animation,LowFrame=s==0?1:0,HighFrame=3-s,Framerate=(s==0?30f:15f)*speed});}
   oracle.FirstCyclic=oracle.AnimList.Last;var node=oracle.AnimList.First;float frame=node.Value.get_starting_frame();var steps=new List<object>();for(int tick=0;tick<100;tick++){var delta=new AFrame();oracle.update_internal(dt,ref node,ref frame,ref delta);steps.Add(new {translation=new[]{delta.Origin.X,delta.Origin.Y,delta.Origin.Z},heading=2f*MathF.Atan2(delta.Orientation.Z,delta.Orientation.W),frame,segment=node==oracle.AnimList.First?0:1});}cases.Add(new{dt,speed,steps});
  }return cases;
 }
 static void Main() {
  var capacity=new List<object>();foreach(var strength in new[]{0,1,10,100,500})foreach(var augs in new[]{0,1,4,5,6,100})foreach(var amount in new[]{0,1,149,15000,30000,45000}){var c=EncumbranceSystem.EncumbranceCapacity(strength,augs);var burden=EncumbranceSystem.GetBurden(c,amount);capacity.Add(new{strength,augs,amount,capacity=c,burden,modifier=EncumbranceSystem.GetBurdenMod(burden)});}
  var runs=new List<object>();var jumps=new List<object>();foreach(var burden in new[]{0f,.5f,.999f,1f,1.5f,1.999f,2f,3f})foreach(var skill in new[]{0,1,100,300,799,800,1000})foreach(var scale in new[]{.5f,1f,2f}){runs.Add(new{burden,skill,scale,rate=(float)MovementSystem.GetRunRate(burden,skill,scale)});foreach(var power in new[]{0f,.25f,1f})foreach(var pk in new[]{false,true})jumps.Add(new{burden,skill,scale,power,pk,height=MovementSystem.GetJumpHeight(burden,(uint)skill,power,scale),cost=MovementSystem.JumpStaminaCost(power,burden,pk)});}
  var controls=new List<object>();foreach(var run in new[]{false,true})foreach(var rate in new[]{1f,2.5f,4.5f})foreach(var input in new uint[]{0x45000005,0x45000006,0x6500000d,0x6500000e,0x6500000f,0x65000010})foreach(var speed in new[]{.25f,1f}) {var m=new MiniInterp();m.WeenieObj.Rate=rate;uint motion=input;float adjusted=speed;m.adjust_motion(ref motion,ref adjusted,run?ACE.Entity.Enum.HoldKey.Run:ACE.Entity.Enum.HoldKey.None);controls.Add(new{run,rate,input,speed,motion,adjusted});}
  Console.WriteLine(JsonSerializer.Serialize(new{capacity,runs,jumps,controls,cmt=Cmt(),contracts=Contracts(),land_info=LandInfo(),land=Land(),root_motion=RootMotions()}));
 }
}
