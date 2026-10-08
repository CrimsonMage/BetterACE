// Synthetic adapters around unmodified ACE methods; AGPL-3.0-only.
using System; using System.Collections.Generic; using System.Globalization; using System.Linq;
static class ThreadSafeRandom {
 public static Queue<double> Floats;public static Queue<int> Integers;public static int F,I;
 public static double Next(float low,float high){F++;return Floats.Dequeue()*(high-low)+low;}
 public static int Next(int low,int high){I++;var value=Integers.Dequeue();if(value<low||value>high)throw new Exception("integer bounds");return value;}
 public static void Set(string floats,string ints){Floats=new(floats.Split(';',StringSplitOptions.RemoveEmptyEntries).Select(double.Parse));Integers=new(ints.Split(';',StringSplitOptions.RemoveEmptyEntries).Select(int.Parse));F=I=0;}
}
static class Rounding {public static int Round(this float value)=>(int)Math.Round(value,MidpointRounding.AwayFromZero);}
class TreasureWielded {public uint WeenieClassId,PaletteId;public float Probability,Shade,StackSizeVariance;public int StackSize;public bool SetStart,HasSubSet,ContinuesPreviousSet;}
record struct ObjectGuid(uint Full) {public static implicit operator ObjectGuid(uint value)=>new(value); public static implicit operator uint(ObjectGuid value)=>value.Full;public override string ToString()=>Full.ToString();}
class WorldObject {
 public ObjectGuid Guid;public uint WeenieClassId,ContainerId;public int? StackSize,MaxStackSize,StackUnitEncumbrance,StackUnitValue,EncumbranceVal,Value,PaletteTemplate;public double? Shade;
 public void CalculateObjDesc(){}
 // STACK
 // WIELDED
 // WALK
 // CREATE
}
class Stackable:WorldObject {}
static class WorldObjectFactory {
 public static WorldObject CreateNewWorldObject(uint id){WorldObject item=id==100?new Stackable():new WorldObject();item.WeenieClassId=id;item.StackSize=3;item.StackUnitEncumbrance=2;item.StackUnitValue=7;item.EncumbranceVal=6;item.Value=21;return item;}
}
class Vendor:WorldObject {
 public Dictionary<uint,WorldObject> DefaultItemsForSale=new();
 // VENDOR
 // MATCH
}
class Program {
 static void Main(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
  var cases=new(string,string,string,string)[]{
   ("empty","","0.9",""),
   ("strict","100:0.5:1:0:0:0:0:0:0;200:0.5:0:0:0:0:0:0:0","0.9;0.5",""),
   ("variance","100:1:1:0:0:5:0.5:8:0.25","0.9;0.1","3"),
   ("nonstackable","200:1:1:0:0:9:0.5:0:0","0.9;0.1","5"),
   ("subset_selected","100:1:1:1:0:0:0:0:0;200:1:1:0:0:0:0:0:0;100:1:1:0:1:0:0:0:0","0.9;0.1;0.8;0.2;0.3",""),
   ("subset_skipped","100:0:1:1:0:0:0:0:0;200:1:1:0:0:0:0:0:0;100:1:1:0:1:0:0:0:0","0.9;0.1;0.8;0.3",""),
   ("implicit_set","100:1:0:0:0:0:0:0:0;200:1:0:0:0:0:0:0:0","0.2;0.3",""),
   ("no_drop","100:0.25:1:0:0:0:0:0:0;200:0.25:0:0:0:0:0:0:0","0.1;0.8",""),
  };
  foreach(var (name,rows,floats,ints) in cases){var table=new List<TreasureWielded>();foreach(var row in rows.Split(';',StringSplitOptions.RemoveEmptyEntries)){var f=row.Split(':');table.Add(new(){WeenieClassId=uint.Parse(f[0]),Probability=float.Parse(f[1]),SetStart=f[2]=="1",HasSubSet=f[3]=="1",ContinuesPreviousSet=f[4]=="1",StackSize=int.Parse(f[5]),StackSizeVariance=float.Parse(f[6]),PaletteId=uint.Parse(f[7]),Shade=float.Parse(f[8])});}ThreadSafeRandom.Set(floats,ints);var result=WorldObject.GenerateWieldedTreasureSets(table);var encoded=string.Join(';',(result??new()).Select(i=>$"{i.WeenieClassId}:{i.StackSize}:{i.PaletteTemplate??0}:{i.Shade??0:R}:{i.EncumbranceVal}:{i.Value}"));Console.WriteLine($"wielded|{name}|{rows}|{floats}|{ints}|{ThreadSafeRandom.F}|{ThreadSafeRandom.I}|{encoded}");}
  foreach(var (name,rows) in new[]{("increment_one","91:100:2:5;92:100:4:5"),("full_then_new","91:100:5:5;92:100:4:5"),("first_inserted","99:100:1:5;10:200:1:5;2:100:1:5;3:100:9:9"),("null_defaults","1:100:-1:-1;2:100:-1:-1"),("fill_stack","1:100:1:3;2:100:1:3;3:100:1:3;4:100:1:3")}){var vendor=new Vendor{Guid=500};foreach(var row in rows.Split(';')){var f=row.Split(':');var item=new Stackable{Guid=uint.Parse(f[0]),WeenieClassId=uint.Parse(f[1]),StackSize=f[2]=="-1"?null:int.Parse(f[2]),MaxStackSize=f[3]=="-1"?null:int.Parse(f[3]),StackUnitEncumbrance=2,StackUnitValue=7};vendor.AddDefaultItem(item);}var result=string.Join(';',vendor.DefaultItemsForSale.Values.Select(i=>$"{i.Guid}:{i.WeenieClassId}:{i.StackSize??-1}:{i.MaxStackSize??-1}"));Console.WriteLine($"vendor|{name}|{rows}|{result}");}
 }
}
