// Lookup/container scaffolding only. Generator inserts original ACE methods.
using System;
using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using System.Threading;
class Frame { public Vector3 Origin; }
class Position {
    public uint Cell; public Frame Frame=new Frame();
    public uint Landblock=>Cell>>16;public byte LandblockX=>(byte)(Cell>>24);public byte LandblockY=>(byte)(Cell>>16);
    // POSITION_METHOD
}
class ObjCell { public List<PhysicsObj> Objects=new List<PhysicsObj>(); public void AddObjectListTo(List<PhysicsObj> destination){destination.AddRange(Objects);} }
class EnvCell:ObjCell {public bool SeenOutside;public Dictionary<uint,EnvCell> VisibleCells=new Dictionary<uint,EnvCell>();}
class Landblock {public List<PhysicsObj> Objects=new List<PhysicsObj>();public List<PhysicsObj> GetServerObjects(bool adjacent)=>Objects;}
class Weenie {public bool IsMonster=false;}
class PhysicsObj {
    public uint ID;public bool DatObject=false;public ObjCell CurCell;public Landblock CurLandblock;public Position Position=new Position();public Weenie WeenieObj=new Weenie();public ObjectMaint ObjMaint;
    public PhysicsObj(uint id){ID=id;ObjMaint=new ObjectMaint(this);}
}
static class PhysicsTimer {public static double CurrentTime;}
class ObjectMaint {
    static ReaderWriterLockSlim rwLock=new ReaderWriterLockSlim(LockRecursionPolicy.SupportsRecursion);
    public PhysicsObj PhysicsObj;public Dictionary<uint,PhysicsObj> KnownObjects=new Dictionary<uint,PhysicsObj>();public Dictionary<uint,PhysicsObj> VisibleObjects=new Dictionary<uint,PhysicsObj>();public Dictionary<PhysicsObj,double> DestructionQueue=new Dictionary<PhysicsObj,double>();
    public const float DestructionTime=25.0f;public static bool InitialClamp=true;public static float InitialClamp_DistSq=112.5f*112.5f;
    public enum VisibleObjectType {All,Players,AttackTargets}
    public ObjectMaint(PhysicsObj obj){PhysicsObj=obj;}
    IEnumerable<PhysicsObj> ApplyFilter(List<PhysicsObj> values,VisibleObjectType type)=>values;
    public void AddVisibleTarget(PhysicsObj obj,bool inverse){}
    public bool RemoveVisibleObject(PhysicsObj obj)=>VisibleObjects.Remove(obj.ID);
    public void RemoveObject(PhysicsObj obj){KnownObjects.Remove(obj.ID);VisibleObjects.Remove(obj.ID);DestructionQueue.Remove(obj);}
    // OBJECT_METHODS
}
static class Program {
    static string Bits(float value)=>BitConverter.SingleToUInt32Bits(value).ToString("X8");
    static Position P(uint cell,float x,float y,float z=0)=>new Position{Cell=cell,Frame=new Frame{Origin=new Vector3(x,y,z)}};
    static PhysicsObj Obj(uint id,Position p,ObjCell cell)=>new PhysicsObj(id){Position=p,CurCell=cell};
    static void Main(){
        var targetCells=new uint[]{0x10100001,0x11100001,0x12100001,0x10100100,0x10100101,0x10100102};
        foreach(uint observerCell in new uint[]{0x10100001,0x10100100,0x10100101}){
            var cells=new Dictionary<uint,ObjCell>();foreach(uint id in targetCells)cells[id]=(id&0xffff)>=0x100?new EnvCell{SeenOutside=id==0x10100101}:new ObjCell();
            ((EnvCell)cells[0x10100100]).VisibleCells[0x102]=(EnvCell)cells[0x10100102];
            ((EnvCell)cells[0x10100101]).VisibleCells[0x100]=(EnvCell)cells[0x10100100];
            var owner=Obj(1,P(observerCell,100,0),cells[observerCell]);owner.CurLandblock=new Landblock();cells[observerCell].Objects.Add(owner);
            for(int i=0;i<targetCells.Length;i++){
                uint cell=targetCells[i];var target=Obj((uint)i+2,P(cell,i,0),cells[cell]);cells[cell].Objects.Add(target);
                int dx=(int)(cell>>24)-(int)(observerCell>>24),dy=(int)((cell>>16)&255)-(int)((observerCell>>16)&255);
                if(Math.Abs(dx)<=1&&Math.Abs(dy)<=1)owner.CurLandblock.Objects.Add(target);
            }
            Console.WriteLine($"pvs,{observerCell:X8},{string.Join(';',owner.ObjMaint.GetVisibleObjects(owner.CurCell).Select(v=>v.ID).OrderBy(v=>v))}");
        }
        foreach(uint a in new uint[]{0x10100001,0x00100001,0xff100001})foreach(uint b in new uint[]{0x10100001,0x11100001,0x00100001})foreach(float x in new float[]{0,112.5f,190.125f}){
            var p=P(a,x,-2.25f,10000);var q=P(b,1.5f,3.25f,-10000);
            Console.WriteLine($"distance,{a:X8},{Bits(p.Frame.Origin.X)},{Bits(p.Frame.Origin.Y)},{b:X8},{Bits(q.Frame.Origin.X)},{Bits(q.Frame.Origin.Y)},{Bits(p.Distance2DSquared(q))}");
        }
        foreach(bool known in new bool[]{false,true})foreach(float distance in new float[]{0,112.5f,MathF.BitIncrement(112.5f),200}){
            var owner=new PhysicsObj(1){Position=P(0x10100001,0,0)};var target=new PhysicsObj(2){Position=P(0x10100001,distance,0,10000)};
            if(known)owner.ObjMaint.KnownObjects[2]=target;
            bool added=owner.ObjMaint.AddVisibleObject(target);
            Console.WriteLine($"clamp,{(known?1:0)},{Bits(owner.Position.Distance2DSquared(target.Position))},{(added?1:0)}");
        }
        foreach(bool repeated in new bool[]{false,true})foreach(int tick in new int[]{749,750,751})foreach(float distance in new float[]{0,200}){
            var owner=new PhysicsObj(1){Position=P(0x10100001,0,0)};var target=new PhysicsObj(2){Position=P(0x10100001,distance,0)};
            owner.ObjMaint.KnownObjects[2]=target;owner.ObjMaint.VisibleObjects[2]=target;PhysicsTimer.CurrentTime=0;owner.ObjMaint.AddObjectToBeDestroyed(target);
            if(repeated){PhysicsTimer.CurrentTime=10;owner.ObjMaint.AddObjectToBeDestroyed(target);}
            PhysicsTimer.CurrentTime=tick/30.0;var removed=owner.ObjMaint.DestroyObjects();bool known=owner.ObjMaint.KnownObjects.ContainsKey(2);bool added=owner.ObjMaint.AddVisibleObject(target);if(added)owner.ObjMaint.KnownObjects[2]=target;owner.ObjMaint.RemoveObjectToBeDestroyed(target);
            Console.WriteLine($"expiry,{(repeated?1:0)},{tick},{Bits(distance*distance)},{removed.Count},{(!known&&added?1:0)},{(owner.ObjMaint.VisibleObjects.ContainsKey(2)?1:0)}");
        }
    }
}
