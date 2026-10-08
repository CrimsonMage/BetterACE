using System; using System.Linq; using System.Collections.Generic;
record struct ObjectGuid(uint Value);
enum SpellId { Vitae=666 } enum EquipmentSet { Set=1 }
record Spell(int Id) { public Spell(int id,bool unused):this(id){} public string Name=>Id.ToString(); }
class Entry { public int SpellId;public double Duration=-1;public uint CasterObjectId;public bool HasSpellSetId;public int SpellSetId=1; }
class Item { public ObjectGuid Guid=new(101);public bool HasItemSet=true;public int EquipmentSetId=1; }
static class Extensions { public static List<Entry> Clone(this List<Entry> rows,object unused)=>rows.ToList(); }
class Biota { public List<Entry> PropertiesEnchantmentRegistry=new(); }
class Manager { public Biota Biota;public void Dispel(Entry entry)=>Biota.PropertiesEnchantmentRegistry.Remove(entry); }
class Log { public void Error(string value){} }
class Player {
 public string Name="Synthetic";public Biota Biota=new();public object BiotaDatabaseLock=new();public Manager EnchantmentManager;Log log=new();
 public Dictionary<ObjectGuid,Item> EquippedObjects=new();public List<Item> possessions=new();public bool active;
 public Player(){EnchantmentManager=new Manager{Biota=Biota};}
 public List<Item> GetAllPossessions()=>possessions;
 public List<Spell> GetSpellSet(List<Item> items)=>active?new(){new(10)}:new();
 public List<Spell> GetSpellSetAll(EquipmentSet set)=>new(){new(10),new(11)};
 // SOURCE_METHOD
}
class Program { static void Main(){for(int n=0;n<16;n++){
 var p=new Player{active=(n&8)!=0};var item=new Item();if((n&4)!=0){p.possessions.Add(item);if((n&2)!=0)p.EquippedObjects[item.Guid]=item;}
 p.Biota.PropertiesEnchantmentRegistry.AddRange(new[]{new Entry{SpellId=10,CasterObjectId=101,HasSpellSetId=(n&1)!=0},new Entry{SpellId=11,CasterObjectId=101,HasSpellSetId=(n&1)!=0},new Entry{SpellId=20,CasterObjectId=999,Duration=60},new Entry{SpellId=666,CasterObjectId=999}});
 p.AuditItemSpells();Console.WriteLine(n+","+string.Join(';',p.Biota.PropertiesEnchantmentRegistry.Select(e=>e.SpellId)));
 }}}
