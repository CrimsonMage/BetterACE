using System;
using System.Collections.Generic;
using System.Globalization;
using ACE.Common.Extensions;
enum PropertyAttribute { Undef=0,Strength,Endurance,Quickness,Coordination,Focus,Self }
enum PropertyAttribute2nd { MaxHealth=1,MaxStamina=3,MaxMana=5 }
namespace DatLoader.Entity { class SkillFormula { public uint X,Attr1,Attr2,Z; } }
class Mods {
 public float Mult=1,Add; public int AttributeAdd;public float AttributeMult=1;
 public float GetVitalMod_Multiplier(CreatureVital v)=>Mult;
 public float GetVitalMod_Additives(CreatureVital v)=>Add;
 public float GetAttributeMod_Multiplier(PropertyAttribute v)=>AttributeMult;
 public int GetAttributeMod_Additive(PropertyAttribute v)=>AttributeAdd;
}
class Creature {public Mods EnchantmentManager=new();public Dictionary<PropertyAttribute,CreatureAttribute> Attributes=new();public DatLoader.Entity.SkillFormula Formula=new();}
class Player:Creature { public int Enlightenment,Gear;public float Vitae=1;public int GetGearMaxHealth()=>Gear; }
class CreatureAttribute {
 public Creature creature;public PropertyAttribute Attribute;public uint Base;public uint Current=>GetCurrent(true);
 // ATTRIBUTE
}
class CreatureVital {
 public Creature creature;public PropertyAttribute2nd Vital;public uint StartingValue,Ranks;
 // VITAL
}
static class AttributeFormula {
 public static uint GetFormula(Creature c,PropertyAttribute2nd vital,bool current)=>GetFormula(c,c.Formula,current);
 // FORMULA
}
class Program {
 static void Main() {
 CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
 foreach(uint start in new uint[]{0,1,4,5,25,100})
 foreach(float vitae in new float[]{1,0.95f,0.6f})
 foreach(float mult in new float[]{0.5f,1,1.1f})
 foreach(float additive in new float[]{-100,-0.5f,0,0.5f,17.5f}) {
 var p=new Player{Enlightenment=2,Gear=7,Vitae=vitae,Formula=new(){X=1,Attr1=2,Attr2=0,Z=2}};
 p.EnchantmentManager.Mult=mult;p.EnchantmentManager.Add=additive;p.EnchantmentManager.AttributeAdd=-7;p.EnchantmentManager.AttributeMult=1.1f;
 for(uint i=1;i<=6;i++)p.Attributes[(PropertyAttribute)i]=new(){creature=p,Attribute=(PropertyAttribute)i,Base=23+i};
 var v=new CreatureVital{creature=p,Vital=PropertyAttribute2nd.MaxHealth,StartingValue=start,Ranks=3};
 Console.WriteLine($"v,{start},3,11,{mult:R},{vitae:R},{additive:R},{v.GetMaxValue(true)}");
 }
 foreach(uint b in new uint[]{0,1,9,10,11,100}) foreach(float m in new float[]{0,0.5f,1,1.1f}) foreach(int add in new int[]{-100,-1,0,7}) {
 var p=new Creature();p.EnchantmentManager.AttributeAdd=add;p.EnchantmentManager.AttributeMult=m;
 var a=new CreatureAttribute{creature=p,Attribute=PropertyAttribute.Endurance,Base=b};Console.WriteLine($"a,{b},{m:R},{add},{a.GetCurrent(true)}");
 }
 }
}
