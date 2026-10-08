using System.Numerics;
using System.Globalization;
public enum GeneratorType {Undefined,Relative,Absolute}
public static class PropertyManager {public static bool Offset=true;public static (bool Item, bool Other) GetBool(string key)=>(Offset,false);}
public partial class SourceProfile {public uint? ObjCellId;public float? OriginX,OriginY,OriginZ,AnglesX,AnglesY,AnglesZ,AnglesW;}
public partial class WorldObject {public GeneratorType GeneratorType;public bool EnterWorld()=>true;}
public partial class GeneratorProfile {public WorldObject Generator;public bool VerifyLandblock(WorldObject o)=>true;public bool VerifyWalkableSlope(WorldObject o)=>true;}
namespace ACE.Entity {
 public class Position {
  public uint Cell;public float PositionX,PositionY,PositionZ;public Quaternion Rotation;
  public Position(uint cell,float x,float y,float z,float qx,float qy,float qz,float qw){Cell=cell;PositionX=x;PositionY=y;PositionZ=z;Rotation=new(qx,qy,qz,qw);}
  public string ToLOCString()=>"";
 }
}
public static class PlacementCases {
 public static void Run(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
  foreach(var name in new[]{"absolute","relative","rotation_absolute","offset_disabled","missing_offset"}){
   PropertyManager.Offset=name!="offset_disabled"&&name!="missing_offset";
   var generator=new WorldObject(){GeneratorType=name=="relative"?GeneratorType.Relative:GeneratorType.Absolute,Location=new(0x01010001,10,20,30,0,0,.70710677f,.70710677f)};
   var p=new GeneratorProfile(){Generator=generator,Biota=new(){ObjCellId=name=="absolute"?0x01010002u:null,OriginX=1,OriginY=name=="missing_offset"?null:2,OriginZ=3,AnglesX=0,AnglesY=0,AnglesZ=0,AnglesW=1}};
   var obj=new WorldObject();p.Spawn_Specific(obj);var l=obj.Location;var q=l.Rotation;
   Console.WriteLine($"{name}|{l.Cell}|{l.PositionX:R},{l.PositionY:R},{l.PositionZ:R}|{q.X:R},{q.Y:R},{q.Z:R},{q.W:R}");
  }
 }
}
