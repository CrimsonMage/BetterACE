using System;using System.IO;using System.Linq;using System.Collections.Generic;using System.Text.Json;
using ACE.Entity.Enum;using ACE.Entity.Models;using ACE.Server.Network;using ACE.Server.Network.Structure;using ACE.Server.Network.GameEvent;using ACE.Server.Network.GameEvent.Events;using ACE.Server.WorldObjects;
namespace ACE.Entity.Models {public class PropertiesEnchantmentRegistry {public int SpellId;public ushort LayerId;}}
namespace ACE.Server.Network {public class Session{}}
namespace ACE.Server.Network.GameEvent {public class GameEventMessage {public MemoryStream Stream=new();public BinaryWriter Writer;public GameEventMessage(GameEventType type,GameMessageGroup group,Session session,int capacity=0){Writer=new BinaryWriter(Stream);Writer.Write(0xf7b0u);Writer.Write(0x50000001u);Writer.Write(17u);Writer.Write((uint)type);}}}
namespace ACE.Server.Network.Structure {
 public class Enchantment {public ushort SpellID,Layer,SpellCategory,HasSpellSetID;public uint PowerLevel,CasterGuid,StatModType,StatModKey,SpellSetID;public double StartTime,Duration,LastTimeDegraded;public float DegradeModifier,DegradeLimit,StatModValue;}
 public class EnchantmentRegistry {public EnchantmentMask EnchantmentMask;public Dictionary<EnchantmentMask,List<Enchantment>> Enchantments=new();}
 public class LayeredSpell {public ushort SpellId,Layer;}
 public static class Writers {// WRITERS
 }
}
namespace ACE.Server.WorldObjects {
 static class ThreadSafeRandom {public static Queue<float> Draws=new();public static float Next(float a,float b)=>Draws.Dequeue();}
 static class Formula {// MANA
 }
}
class Component {public float CDM;}
class ComponentTable {public Dictionary<uint,Component> SpellComponents=new(){{1,new(){CDM=0.5f}},{63,new(){CDM=2.0f}}};}
class SpellFormula {public static ComponentTable SpellComponentsTable=new();public List<uint> CurrentFormula=new(){1,63,1};}
class Player {public uint Skill;public CreatureSkill GetCreatureSkill(int ignored)=>new(){Current=Skill};}
class CreatureSkill {public uint Current;}
class BurnSpell {public float ComponentLoss;public uint Power;public SpellFormula Formula=new();public int GetMagicSkill()=>1;
// BURN
}
class Program {
 static string Bytes(Action<BinaryWriter> write){using var s=new MemoryStream();using var w=new BinaryWriter(s);write(w);return Convert.ToHexString(s.ToArray()).ToLowerInvariant();}
 static Enchantment Entry(ushort spell=100,ushort layer=4,ushort set=1)=>new(){SpellID=spell,Layer=layer,SpellCategory=37,HasSpellSetID=set,PowerLevel=250,StartTime=-15.0,Duration=120.5,CasterGuid=0x50000001,DegradeModifier=0.25f,DegradeLimit=-666f,LastTimeDegraded=0,StatModType=0x02004008,StatModKey=64,StatModValue=1.125f,SpellSetID=17};
 static void Main(){var entries=new[]{Entry(),Entry(666,99),Entry(101,1,0),Entry(102,2,2)};var enchants=entries.Select(e=>Bytes(w=>w.Write(e))).ToArray();
 var registries=new List<object>();for(int mask=0;mask<16;mask++){var r=new EnchantmentRegistry{EnchantmentMask=(EnchantmentMask)mask};r.Enchantments[EnchantmentMask.Multiplicative]=new(){entries[0]};r.Enchantments[EnchantmentMask.Additive]=new(){entries[2]};r.Enchantments[EnchantmentMask.Cooldown]=new(){entries[3]};r.Enchantments[EnchantmentMask.Vitae]=new(){entries[1]};registries.Add(new{mask,bytes=Bytes(w=>w.Write(r))});}
 var session=new Session();var layered=new List<LayeredSpell>{new(){SpellId=100,Layer=1},new(){SpellId=65000,Layer=65535}};
 GameEventMessage[] events={new GameEventMagicUpdateSpell(session,100,2),new GameEventMagicUpdateEnchantment(session,entries[0]),new GameEventMagicRemoveEnchantment(session,100,2),new GameEventMagicUpdateMultipleEnchantments(session,entries.ToList()),new GameEventMagicRemoveMultipleEnchantments(session,layered),new GameEventMagicPurgeEnchantments(session),new GameEventMagicDispelEnchantment(session,65000,65535),new GameEventMagicDispelMultipleEnchantments(session,layered),new GameEventMagicPurgeBadEnchantments(session)};
 var mana=new List<object>();foreach(uint skill in new uint[]{0,1,50,100,300,1000})foreach(uint difficulty in new uint[]{0,50,100,300})foreach(uint cost in new uint[]{0,1,25,100})foreach(float draw in new[]{0f,0.5f,0.99999994f}){ThreadSafeRandom.Draws=new(new[]{draw,0.25f,0.75f});uint value=Formula.GetManaCost(difficulty,cost,skill);mana.Add(new{skill,difficulty,cost,draw,value,consumed=3-ThreadSafeRandom.Draws.Count});}
 var chance=new List<object>();foreach(int skill in new[]{0,50,100,500})foreach(int difficulty in new[]{0,50,100,500})chance.Add(new{skill,difficulty,value=SkillCheck.GetMagicSkillChance(skill,difficulty)});
 var burns=new List<object>();foreach(uint power in new uint[]{0,100,300})foreach(uint skill in new uint[]{1,100,300})foreach(float loss in new[]{0f,0.25f,1f}){ThreadSafeRandom.Draws=new(new[]{0.1f,0.5f,0.9f});var spell=new BurnSpell{Power=power,ComponentLoss=loss};burns.Add(new{power,skill,loss,consumed=spell.TryBurnComponents(new Player{Skill=skill})});}
 Console.WriteLine(JsonSerializer.Serialize(new{burns,enchantments=enchants,registries,events=events.Select(e=>new{name=e.GetType().Name,bytes=Convert.ToHexString(e.Stream.ToArray()).ToLowerInvariant()}),mana,chance}));}
}
