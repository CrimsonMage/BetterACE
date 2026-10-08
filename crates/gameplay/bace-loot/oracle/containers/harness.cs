// Boundary stubs; the three Container methods are injected unchanged.
using System.Text.Json;
using ACE.Entity.Enum;
namespace ACE.Entity.Enum {public enum DestinationType{Contain=1,Wield=2,ContainTreasure=9} public enum Placement:uint {Default=0,Resting=101}}
public record struct ObjectGuid(uint Full){public bool IsPlayer()=>false;}
public class Row {public uint WeenieClassId;public DestinationType DestinationType;public int Palette;public float Shade;public int StackSize;}
public class Biota {public List<Row> PropertiesCreateList=new();}
public class WorldObject {
 public ObjectGuid Guid;public uint WeenieClassId;public uint? GeneratorId;
 public int PaletteTemplate=2;public double Shade=.75;public int StackSize=1;
 public bool UseBackpackSlot;public int? EncumbranceVal=0,Value=0;public object Location;
 public Placement? Placement;public uint OwnerId,ContainerId;public Container Container;public int PlacementPosition;
 public void SetStackSize(int v){StackSize=v;}
}
public class Container:WorldObject {
 public Biota Biota=new();public int? ContainerCapacity,ItemCapacity;
 public Dictionary<ObjectGuid,WorldObject> Inventory=new();public void OnAddItem(){}
 /*CONTAIN*/
 /*ADD1*/
 /*ADD2*/
}
public class Player:Container{public bool HasEnoughBurdenToAddToInventory(WorldObject w)=>true;}
public static class WorldObjectFactory {
 public static Dictionary<uint,JsonElement> Templates=new();public static Dictionary<uint,uint> IdToTemplate=new();public static uint Next;
 public static WorldObject CreateNewWorldObject(uint id){
  if(!Templates.TryGetValue(id,out var t))return null;
  var type=t.GetProperty("type").GetUInt32();var w=type is /*CONTAINER_TYPES*/?new Container():new WorldObject();
  w.Guid=new ObjectGuid(++Next);IdToTemplate[w.Guid.Full]=id;w.WeenieClassId=id;w.UseBackpackSlot=t.GetProperty("side").GetBoolean();
  if(w is Container c){c.ItemCapacity=t.GetProperty("main").GetInt32();c.ContainerCapacity=t.GetProperty("pack").GetInt32();foreach(var r in t.GetProperty("rows").EnumerateArray())c.Biota.PropertiesCreateList.Add(new Row{WeenieClassId=r.GetProperty("id").GetUInt32(),DestinationType=(DestinationType)r.GetProperty("dest").GetInt32(),Palette=r.GetProperty("palette").GetInt32(),Shade=r.GetProperty("shade").GetSingle(),StackSize=r.GetProperty("stack").GetInt32()});c.GenerateContainList();}
  return w;
 }
}
public static class Program {
 static void Flatten(WorldObject w,List<object> result,uint? parent){result.Add(new {id=w.WeenieClassId,parent,generator=w.GeneratorId.HasValue?(uint?)WorldObjectFactory.IdToTemplate[w.GeneratorId.Value]:null,slot=w.PlacementPosition,palette=w.PaletteTemplate,shade=w.Shade,stack=w.StackSize});if(w is Container c)foreach(var child in c.Inventory.Values)Flatten(child,result,w.WeenieClassId);}
 public static void Main(){var cases=new List<object>();foreach(var input in JsonDocument.Parse(File.ReadAllText("inputs.json")).RootElement.EnumerateArray()){WorldObjectFactory.Next=0;WorldObjectFactory.IdToTemplate.Clear();WorldObjectFactory.Templates=input.EnumerateArray().ToDictionary(t=>t.GetProperty("id").GetUInt32(),t=>t.Clone());var root=WorldObjectFactory.CreateNewWorldObject(1);var output=new List<object>();Flatten(root,output,null);cases.Add(new {input=input.Clone(),output});}Console.WriteLine(JsonSerializer.Serialize(new {cases}));}
}
