using System.Globalization;
using System.Reflection;
using ACE.Entity.Enum;
using ACE.Entity.Enum.Properties;
using ACE.Server.Entity.Mutations;
using ACE.Server.WorldObjects;
namespace log4net {public interface ILog{void Error(object o);void Debug(object o);} public class Log:ILog{public void Error(object o){} public void Debug(object o){}} public static class LogManager{public static ILog GetLogger(Type t)=>new Log();}}
namespace ACE.Common {public static class ThreadSafeRandom{public static double Next(float min,float max)=>min+(max-min)*0.25;}}
namespace ACE.Server.Factories.Enum { public class Boundary {} }
namespace ACE.Server.WorldObjects {
 public class WorldObject {
  public string Name="synthetic"; public uint Guid=1;
  public Dictionary<PropertyInt,int> Ints=new(); public Dictionary<PropertyInt64,long> Longs=new(); public Dictionary<PropertyFloat,double> Floats=new(); public Dictionary<PropertyBool,bool> Bools=new(); public Dictionary<PropertyDataId,uint> Dids=new();
  public int? GetProperty(PropertyInt k)=>Ints.TryGetValue(k,out var v)?v:null;
  public long? GetProperty(PropertyInt64 k)=>Longs.TryGetValue(k,out var v)?v:null;
  public double? GetProperty(PropertyFloat k)=>Floats.TryGetValue(k,out var v)?v:null;
  public bool? GetProperty(PropertyBool k)=>Bools.TryGetValue(k,out var v)?v:null;
  public uint? GetProperty(PropertyDataId k)=>Dids.TryGetValue(k,out var v)?v:null;
  public void SetProperty(PropertyInt k,int v)=>Ints[k]=v;public void SetProperty(PropertyInt64 k,long v)=>Longs[k]=v;public void SetProperty(PropertyFloat k,double v)=>Floats[k]=v;public void SetProperty(PropertyBool k,bool v)=>Bools[k]=v;public void SetProperty(PropertyDataId k,uint v)=>Dids[k]=v;
  public string Dump()=>string.Join(";",Ints.OrderBy(p=>(int)p.Key).Select(p=>$"I:{(int)p.Key}:{p.Value}").Concat(Longs.OrderBy(p=>(int)p.Key).Select(p=>$"L:{(int)p.Key}:{p.Value}")).Concat(Floats.OrderBy(p=>(int)p.Key).Select(p=>$"F:{(int)p.Key}:{BitConverter.DoubleToUInt64Bits(p.Value):x16}")).Concat(Bools.OrderBy(p=>(int)p.Key).Select(p=>$"B:{(int)p.Key}:{(p.Value?1:0)}")).Concat(Dids.OrderBy(p=>(int)p.Key).Select(p=>$"D:{(int)p.Key}:{p.Value}")));
 }
}
class Program {
 static void Main(){
  CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
  foreach(var file in Assembly.GetExecutingAssembly().GetManifestResourceNames().Order()) {
   var id=Convert.ToUInt32(file.Split('.')[^2][..8],16);var script=MutationCache.GetMutation(id);
   if(script==null)throw new Exception(file);
   var keys=script.Mutations.SelectMany(m=>m.Outcomes).SelectMany(o=>o.EffectLists).SelectMany(l=>l.Effects).SelectMany(e=>new[]{e.Quality,e.Arg1,e.Arg2}).Where(a=>a!=null&&a.Type==EffectArgumentType.Quality).DistinctBy(a=>(a.StatType,a.StatIdx)).ToArray();
   for(int seed=0;seed<9;seed++) {
    var wo=new WorldObject();
    if(seed>0) foreach(var k in keys){
     var integer=new[]{0,0,1,-1,49,50,99,500,2000000000}[seed];var floating=new[]{0.0,0.0,0.01,-0.01,0.009999,0.1,1.0,1.2,10.0}[seed];
     switch(k.StatType){case StatType.Int:wo.SetProperty((PropertyInt)k.StatIdx,integer);break;case StatType.Int64:wo.SetProperty((PropertyInt64)k.StatIdx,(long)integer);break;case StatType.Float:wo.SetProperty((PropertyFloat)k.StatIdx,floating);break;case StatType.Bool:wo.SetProperty((PropertyBool)k.StatIdx,seed%2==0);break;case StatType.DID:wo.SetProperty((PropertyDataId)k.StatIdx,(uint)Math.Max(0,integer));break;}
    }
    // Avoid intentionally undefined/overflowing source arithmetic vectors.
    if(seed==8 && id is 0x3800001f or 0x38000035 or 0x38000046)continue;
    var before=wo.Dump();script.TryMutate(wo);Console.WriteLine($"{id:x8}\t{seed}\t{before}\t{wo.Dump()}");
   }
  }
 }
}
