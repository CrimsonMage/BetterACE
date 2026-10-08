using ACE.Entity.Enum;using ACE.Common;
namespace ACE.Common{public static class ThreadSafeRandom{public static double Value;public static int Draws;public static double Next(float min,float max){Draws++;return Value*(max-min)+min;}public static int Next(int min,int max){Draws++;return min+(int)(Value*(max-min+1));}}}
public class Row{public uint MaterialId;public float Probability;}
public class TreasureMaterialColor{public uint PaletteTemplate;public float Probability;}
public static class DatabaseManager{public static WorldDatabase World=new();}
public class WorldDatabase{public List<Row> Base,Group;public List<TreasureMaterialColor> Colors;public List<Row> GetCachedTreasureMaterialBase(int code,int tier)=>Base;public List<Row> GetCachedTreasureMaterialGroup(int code,int tier)=>Group;public List<TreasureMaterialColor> GetCachedTreasureMaterialColors(int material,byte code)=>Colors;}
namespace DatLoader.FileTypes {public class Effect{public uint Icon;}public class ClothingTable{public Dictionary<uint,Effect> ClothingSubPalEffects=new();}}
namespace DatLoader {public static class DatManager{public static Archive PortalDat=new();}public class Archive{public FileTypes.ClothingTable Table=new();public T ReadFromDat<T>(uint id)=>(T)(object)Table;}}
public class WorldObject{public string Name="oracle";public uint WeenieClassId=1;public int? TsysMutationData;public MaterialType? MaterialType;public WeenieType WeenieType;public ItemType ItemType;public uint? ClothingBase;public uint IconId;public int PaletteTemplate;public double Shade;}
public class Log{public void Warn(object message){}}
// DEFAULT
public class Oracle{
static Log log=new();static List<TreasureMaterialColor> clothingColors=Enumerable.Range(1,18).Select(i=>new TreasureMaterialColor{PaletteTemplate=(uint)i,Probability=1}).ToList();
// METHODS
public static void Main(){foreach(var scenario in Enumerable.Range(0,8))foreach(var tier in new[]{1,8})foreach(var draw in new[]{0d,.1d,.5d,.999999d}){
 var w=new WorldObject{WeenieType=WeenieType.Clothing,ItemType=ItemType.Armor,TsysMutationData=1,MaterialType=MaterialType.Copper,ClothingBase=7};DatabaseManager.World.Base=new(){new Row{MaterialId=(uint)MaterialType.Ivory,Probability=.2f},new Row{MaterialId=1,Probability=.8f}};DatabaseManager.World.Group=new(){new Row{MaterialId=(uint)MaterialType.Gold,Probability=.3f},new Row{MaterialId=(uint)MaterialType.Silver,Probability=.7f}};DatabaseManager.World.Colors=Enumerable.Range(1,10).Select(i=>new TreasureMaterialColor{PaletteTemplate=(uint)i,Probability=.1f}).ToList();DatLoader.DatManager.PortalDat.Table.ClothingSubPalEffects=Enumerable.Range(1,10).ToDictionary(i=>(uint)i,i=>new DatLoader.FileTypes.Effect{Icon=(uint)(100+i)});
 if(scenario==1)w.TsysMutationData=null;if(scenario==2)DatabaseManager.World.Base=null;if(scenario==3)DatabaseManager.World.Group=null;if(scenario==4)DatabaseManager.World.Colors=null;if(scenario==5){DatabaseManager.World.Colors=null;w.TsysMutationData=1|(1<<16);}if(scenario==6)DatLoader.DatManager.PortalDat.Table.ClothingSubPalEffects.Clear();if(scenario==7)foreach(var effect in DatLoader.DatManager.PortalDat.Table.ClothingSubPalEffects.Values)effect.Icon=0;
 ThreadSafeRandom.Value=draw;ThreadSafeRandom.Draws=0;w.MaterialType=GetMaterialType(w,tier);MutateColor(w);Console.WriteLine($"{scenario}|{tier}|{draw:R}|{(int)w.MaterialType}|{w.IconId}|{w.PaletteTemplate}|{w.Shade:R}|{ThreadSafeRandom.Draws}");
}}}
