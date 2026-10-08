using System;using System.Linq;using System.Collections.Generic;
using ACE.Entity.Enum;using ACE.Entity.Models;using ACE.DatLoader;using ACE.DatLoader.FileTypes;using ACE.DatLoader.Entity;
namespace ACE.Entity.Models {public class PropertiesPalette{public uint SubPaletteId;public ushort Offset,Length;}public class PropertiesAnimPart{public byte Index;public uint AnimationId;}public class PropertiesTextureMap{public byte PartIndex;public uint OldTexture,NewTexture;}}
namespace ACE.DatLoader.Entity {
 public class CloTextureEffect{public uint OldTexture,NewTexture;}
 public class CloObjectEffect{public uint Index,ModelId;public List<CloTextureEffect>CloTextureEffects=new();}
 public class ClothingBaseEffect{public List<CloObjectEffect>CloObjectEffects=new();}
 public class CloSubPalEffect{public uint Icon;public List<CloSubPalette>CloSubPalettes=new();}
 public class CloSubPalette{public uint PaletteSet;public List<Range>Ranges=new();}
 public class Range{public uint Offset,NumColors;}
 public class SexCG{public List<HairStyleCG>HairStyleList=new();}
 public class HairStyleCG{public StyleDesc ObjDesc=new();}
 public class StyleDesc{public List<StyleTexture>TextureChanges=new();public List<StylePart>AnimPartChanges=new();}
 public class StyleTexture{public byte PartIndex;public uint OldTexture,NewTexture;}
 public class StylePart{public byte PartIndex;public uint PartID;}
 public class Heritage{public Dictionary<int,SexCG>Genders=new();}
 public class CharGen{public Dictionary<uint,Heritage>HeritageGroups=new();}
}
namespace ACE.DatLoader.FileTypes {
 public class SetupModel{public List<uint>Parts=new();}
 public class PaletteSet{public List<uint>PaletteList=new(); PALETTE_METHOD}
 public class ClothingTable{public Dictionary<uint,ClothingBaseEffect>ClothingBaseEffects=new();public Dictionary<uint,CloSubPalEffect>ClothingSubPalEffects=new(); PRIORITY_METHOD}
}
namespace ACE.DatLoader {public static class DatManager{public static Portal PortalDat=new();}public class Portal{public CharGen CharGen=new();public Dictionary<uint,object>Data=new();public T ReadFromDat<T>(uint id)=>(T)Data[id];}}
class LockedList<T>:List<T>{public int GetCount(object x)=>Count;public void CopyTo(List<T>x,object l)=>x.AddRange(this);}
class Biota{public LockedList<PropertiesAnimPart>PropertiesAnimPart=new();public LockedList<PropertiesTextureMap>PropertiesTextureMap=new();public LockedList<PropertiesPalette>PropertiesPalette=new();}
class Character{public uint DefaultHairTexture=0x05000101,HairTexture=0x05000102;}
class WorldObject{
 public uint SetupTableId=0x02000001;public uint? ClothingBase,HeadObjectDID,HairPaletteDID,PaletteBaseDID,SkinPaletteDID,EyesTextureDID,DefaultEyesTextureDID,NoseTextureDID,DefaultNoseTextureDID,MouthTextureDID,DefaultMouthTextureDID,EyesPaletteDID;public int? HairStyle,Heritage,Gender,PaletteTemplate;public float? Shade;public bool?IgnoreCloIcons,TopLayerPriority;public uint IconId;public ItemType ItemType;public EquipMask CurrentWieldedLocation;public CoverageMask? ClothingPriority,VisualClothingPriority;public SetupModel CSetup=>DatManager.PortalDat.ReadFromDat<SetupModel>(SetupTableId);
 public Biota Biota=new();public object BiotaDatabaseLock=new();public void setVisualClothingPriority(){if(ClothingBase.HasValue && (CurrentWieldedLocation&(EquipMask.Armor|EquipMask.Extremity))!=0)VisualClothingPriority=DatManager.PortalDat.ReadFromDat<ClothingTable>(ClothingBase.Value).GetVisualPriority()??ClothingPriority;}
 BASE_METHOD
 WORLD_METHOD
}
class Hook:WorldObject{public bool HasItem;public WorldObject Item;}
class Creature:WorldObject{public Dictionary<uint,WorldObject>EquippedObjects=new(); CREATURE_METHOD SETUP_METHOD }
class Player:Creature{public Character Character=new();public bool Helm=true,Cloak=true;public bool GetCharacterOption(CharacterOption o)=>o==CharacterOption.ShowYourHelmOrHeadGear?Helm:Cloak;}
class Program{
 static WorldObject Gear(uint id,EquipMask loc,ItemType type,int priority,bool?top=null){return new WorldObject{ClothingBase=id,CurrentWieldedLocation=loc,ItemType=type,ClothingPriority=(CoverageMask)priority,TopLayerPriority=top,PaletteTemplate=999,Shade=0.6f};}
 static ClothingTable Table(uint model,uint part){var t=new ClothingTable();t.ClothingBaseEffects[0x02000001]=new ClothingBaseEffect{CloObjectEffects=new(){new CloObjectEffect{Index=part,ModelId=model,CloTextureEffects=new(){new CloTextureEffect{OldTexture=0x05000011,NewTexture=model+0x04000000}}}}};t.ClothingSubPalEffects[7]=new CloSubPalEffect{Icon=0x06000077,CloSubPalettes=new(){new CloSubPalette{PaletteSet=0x0f000001,Ranges=new(){new ACE.DatLoader.Entity.Range{Offset=64,NumColors=16}}}}};t.ClothingSubPalEffects[1]=new CloSubPalEffect{Icon=0x06000011};return t;}
 static void Main(){var dat=DatManager.PortalDat;var setup=new SetupModel{Parts=Enumerable.Range(1,17).Select(x=>0x01000000u+(uint)x).ToList()};dat.Data[0x02000001]=setup;dat.Data[0x02001aa3]=setup;dat.Data[0x02000002]=new SetupModel{Parts=new(){0x01000099,0x01000098}};dat.Data[0x0f000001]=new PaletteSet{PaletteList=new(){0x04000012,0x04000034,0x04000056}};dat.Data[0x10000001]=Table(0x01000081,9);dat.Data[0x10000002]=Table(0x01000082,16);dat.Data[0x10000003]=Table(0x01000083,9);
 dat.CharGen.HeritageGroups[6]=new Heritage{Genders=new(){{1,new SexCG{HairStyleList=new(){new HairStyleCG{ObjDesc=new StyleDesc{AnimPartChanges=new(){new StylePart{PartIndex=16,PartID=0x010000dd}},TextureChanges=new(){new StyleTexture{PartIndex=16,OldTexture=0x050000dd,NewTexture=0x050000ee}}}}}}}}};
 for(int c=0;c<16;c++){
 var p=new Player{Heritage=c==14?6:1,Gender=1,HeadObjectDID=0x01000070,PaletteBaseDID=0x04000001,HairPaletteDID=0x04000002,SkinPaletteDID=0x04000003,EyesPaletteDID=0x04000004,DefaultEyesTextureDID=0x05000001,EyesTextureDID=0x05000002,DefaultNoseTextureDID=0x05000003,NoseTextureDID=0x05000004,DefaultMouthTextureDID=0x05000005,MouthTextureDID=0x05000006};
 if(c==14)p.HairStyle=0;
 if(c==1){p.Biota.PropertiesAnimPart.Add(new PropertiesAnimPart{Index=2,AnimationId=0x010000aa});p.Biota.PropertiesPalette.Add(new PropertiesPalette{SubPaletteId=0x04000055,Offset=2,Length=3});}
 if(c>=2 && c!=14){p.EquippedObjects[1]=Gear(0x10000001,EquipMask.ChestArmor,ItemType.Armor,4);p.EquippedObjects[2]=Gear(0x10000002,EquipMask.HeadWear,ItemType.Clothing,1);}
 if(c==3)p.Helm=false;
 if(c>=4 && c<=8){p.EquippedObjects[3]=Gear(0x10000003,EquipMask.Cloak,ItemType.Clothing,8);p.EquippedObjects[1].TopLayerPriority=c==5?true:c==6?false:null;p.Cloak=c!=7;}
 if(c==8)p.EquippedObjects[1].PaletteTemplate=1;
 if(c==9)p.SetupTableId=0x02001aa3;
 if(c==10)p.EquippedObjects[1].ClothingBase=null;
 if(c==10)p.EquippedObjects[1].SetupTableId=0x02000002;
 if(c==11)p.EquippedObjects[1].Shade=1;
 if(c==12)p.EquippedObjects[1].Shade=-1;
 if(c==13)p.EquippedObjects[1].Shade=0;
 if(c==15){p.EquippedObjects.Clear();p.ClothingBase=0x10000001;p.Shade=.6f;}
 Emit(c,p.CalculateObjDesc(),p.IconId);
 }
 for(int c=16;c<20;c++){var w=Gear(0x10000001,EquipMask.ChestArmor,ItemType.Armor,4);w.PaletteBaseDID=0x04000001;if(c==17)w.IgnoreCloIcons=true;if(c==18){w.Shade=null;w.PaletteTemplate=null;}if(c==19)w.SetupTableId=0x02000002;Emit(c,w.CalculateObjDesc(),w.IconId);}
 }
 static void Emit(int c,ACE.Entity.ObjDesc d,uint icon){Console.WriteLine(c+"|"+icon+"|"+d.PaletteID+"|"+string.Join(";",d.AnimPartChanges.Select(x=>$"{x.Index},{x.AnimationId}"))+"|"+string.Join(";",d.TextureChanges.Select(x=>$"{x.PartIndex},{x.OldTexture},{x.NewTexture}"))+"|"+string.Join(";",d.SubPalettes.Select(x=>$"{x.SubPaletteId},{x.Offset},{x.Length}")));}
}
namespace ACE.Entity.Enum{public static class AttributeLookup{public static T GetAttributeOfType<T>(this System.Enum value) where T:Attribute=>value.GetType().GetField(value.ToString()).GetCustomAttributes(typeof(T),false).Cast<T>().FirstOrDefault();}}
