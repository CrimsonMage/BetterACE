using System.Globalization;
using ACE.Entity.Enum;
using ACE.Entity.Enum.Properties;
class GuidValue {public uint Full; public GuidValue(uint value){Full=value;}}
class Book {public HashSet<int> Spells=new(); public void GetOrAddKnownSpell(int id,object gate,out bool added)=>added=Spells.Add(id);}
class WorldObject {
 public GuidValue Guid; public string Name;public bool ChangesDetected;public object BiotaDatabaseLock=new();public Book Biota=new();
 public Dictionary<int,int> Ints=new();public Dictionary<int,double> Floats=new();public Dictionary<int,bool> Bools=new();public Dictionary<int,uint> Dids=new(),Iids=new();public Dictionary<int,string> Strings=new();
 public WorldObject(uint id){Guid=new(id);Name=$"item{id}";Strings[1]=Name;}
 public int? GetProperty(PropertyInt k)=>Ints.TryGetValue((int)k,out var v)?v:null;
 public double? GetProperty(PropertyFloat k)=>Floats.TryGetValue((int)k,out var v)?v:null;
 public bool? GetProperty(PropertyBool k)=>Bools.TryGetValue((int)k,out var v)?v:null;
 public uint? GetProperty(PropertyDataId k)=>Dids.TryGetValue((int)k,out var v)?v:null;
 public uint? GetProperty(PropertyInstanceId k)=>Iids.TryGetValue((int)k,out var v)?v:null;
 public string GetProperty(PropertyString k)=>Strings.TryGetValue((int)k,out var v)?v:null;
 public string Dump()=>string.Join(";",Ints.OrderBy(p=>p.Key).Select(p=>$"I:{p.Key}:{p.Value}").Concat(Floats.OrderBy(p=>p.Key).Select(p=>$"F:{p.Key}:{BitConverter.DoubleToUInt64Bits(p.Value):x16}")).Concat(Bools.OrderBy(p=>p.Key).Select(p=>$"B:{p.Key}:{(p.Value?1:0)}")).Concat(Dids.OrderBy(p=>p.Key).Select(p=>$"D:{p.Key}:{p.Value}")).Concat(Iids.OrderBy(p=>p.Key).Select(p=>$"G:{p.Key}:{p.Value}")).Concat(Strings.OrderBy(p=>p.Key).Select(p=>$"S:{p.Key}:{p.Value}")).Concat(Biota.Spells.Order().Select(id=>$"P:{id}:1")));
}
class Player:WorldObject {
 public Player(uint id):base(id){}public Session Session=new();
 public void UpdateProperty(WorldObject o,PropertyInt k,int v)=>o.Ints[(int)k]=v;
 public void UpdateProperty(WorldObject o,PropertyFloat k,double v)=>o.Floats[(int)k]=v;
 public void UpdateProperty(WorldObject o,PropertyBool k,bool v)=>o.Bools[(int)k]=v;
 public void UpdateProperty(WorldObject o,PropertyDataId k,uint v)=>o.Dids[(int)k]=v;
 public void UpdateProperty(WorldObject o,PropertyInstanceId k,uint v)=>o.Iids[(int)k]=v;
 public void UpdateProperty(WorldObject o,PropertyString k,string v){if(v==null)o.Strings.Remove((int)k);else o.Strings[(int)k]=v;}
}
class Session{public Network Network=new();}class Network{public void EnqueueSend(object o){}}class GameMessageSystemChat{public GameMessageSystemChat(string s,ChatMessageType t){}}enum ChatMessageType{Craft}
class Log{public void Warn(object o){}}
/*ROW_TYPES*/
class Program {
 static Log log=new();static bool Debug=false;
 /*METHODS*/
 static void Main(){
  CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
  for(int operation=0;operation<=10;operation++)foreach(double? prop in new double?[]{null,-1,0,1,2,3,7,.3,-.3,2147483648,4294967295})foreach(double value in new double[]{-1,0,1,2,3,7,.3,2147483648,4294967295}) {
   var ok=VerifyRequirement(new Player(1),(CompareType)operation,prop,value,"");Console.WriteLine($"R\t{operation}\t{(prop.HasValue?prop.Value.ToString("R"):"null")}\t{value:R}\t{(ok?1:0)}");
  }
  for(int operation=4;operation<=8;operation++)foreach(string prop in new string[]{null,"","alpha","beta"})foreach(string value in new[]{"","alpha","beta"}) {
   var ok=VerifyRequirement(new Player(1),(CompareType)operation,prop,value,"");Console.WriteLine($"T\t{operation}\t{prop??"null"}\t{value}\t{(ok?1:0)}");
  }
  foreach(string family in new[]{"Bool","Int","Float","String","IID","DID"})for(int index=0;index<8;index++)for(int source=0;source<2;source++)for(int seed=0;seed<3;seed++)foreach(int operation in new[]{1,2,3,7,8,9}) {
   if(family=="Bool"&&operation!=1||family=="Float"&&operation!=1&&operation!=2&&operation!=3||family is "String" or "IID" or "DID"&&operation!=1&&operation!=3)continue;
   var p=new Player(1);var s=new WorldObject(2);var t=new WorldObject(3);
   int stat=family=="String"?16:family=="IID"?(seed==2?38:seed==1?30:31):123;
   if(seed>0)foreach(var obj in new WorldObject[]{p,s,t}) {
    int n=(int)obj.Guid.Full*10;obj.Ints[stat]=n;obj.Floats[stat]=n+.125;obj.Bools[stat]=true;obj.Strings[stat]=$"saved{n}";obj.Dids[stat]=(uint)n;obj.Iids[stat]=(uint)n;
   }
   var modified=new HashSet<uint>();
   switch(family){
    case "Bool":ModifyBool(p,new(){Index=(sbyte)index,Stat=stat,Source=source,Enum=operation,Value=false},s,t,null,modified);break;
    case "Int":ModifyInt(p,new(){Index=(sbyte)index,Stat=stat,Source=source,Enum=operation,Value=5},s,t,null,modified);break;
    case "Float":ModifyFloat(p,new(){Index=(sbyte)index,Stat=stat,Source=source,Enum=operation,Value=.3},s,t,null,modified);break;
    case "String":ModifyString(p,new(){Index=(sbyte)index,Stat=stat,Source=source,Enum=operation,Value=seed==2?null:"literal"},s,t,null,modified);break;
    case "IID":ModifyInstanceID(p,new(){Index=(sbyte)index,Stat=stat,Source=source,Enum=operation,Value=50},s,t,null,modified);break;
    case "DID":ModifyDataID(p,new(){Index=(sbyte)index,Stat=stat,Source=source,Enum=operation,Value=50},s,t,null,modified);break;
   }
   Console.WriteLine($"M\t{family}\t{index}\t{source}\t{seed}\t{operation}\t{stat}\t{p.Dump()}\t{s.Dump()}\t{t.Dump()}");
  }
 }
}
