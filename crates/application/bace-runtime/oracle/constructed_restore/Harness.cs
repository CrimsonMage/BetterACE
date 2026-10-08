// BetterACE oracle scaffolding, AGPL-3.0-only. Extracted ACE bodies retain their
// upstream authorship/license; this harness supplies explicitly traced helpers.
using System;
using System.Linq;
using System.Collections.Generic;
using System.Text.Json;
public enum PropertyAttribute { Strength,Endurance,Coordination,Quickness,Focus,Self }
public enum PropertyAttribute2nd { MaxHealth,MaxStamina,MaxMana }
public enum CombatBodyPart { Head }
public enum Skill { One,Two }
public enum PropertyInt { EncumbranceVal,Value }
public enum CombatMode { NonCombat,Combat }
public enum MotionStance { NonCombat }
public enum MotionCommand { Ready }
[Flags] public enum EquipMask { None=0,One=1 }
public record struct ObjectGuid(uint Full);
public class PropertiesAttribute {}
public class PropertiesAttribute2nd {}
public class PropertiesBodyPart {}
public class PropertiesSkill {}
public class WorldObjectInfo {}
public class Biota {
    public uint Id=10;
    public int Initial;
    public bool Existing,Fail,Attackable;
    public Dictionary<PropertyAttribute,PropertiesAttribute> PropertiesAttribute;
    public Dictionary<PropertyAttribute2nd,PropertiesAttribute2nd> PropertiesAttribute2nd;
    public Dictionary<CombatBodyPart,PropertiesBodyPart> PropertiesBodyPart;
    public Dictionary<Skill,PropertiesSkill> PropertiesSkill;
    public int[] Spells=Array.Empty<int>();
    public IEnumerable<int> GetKnownSpellsIds(object ignored)=>Spells;
}
public class Weenie { public Biota Data; }
public static class Trace {
    public static List<string> Values=new();
    public static void Add(string value)=>Values.Add(value);
}
public class WorldObject {
    public Biota Biota;
    public ObjectGuid Guid;
    public EquipMask? ValidLocations=EquipMask.One;
    public WorldObject(Biota data){Biota=data;Guid=new(data.Id);}
}
public class Container:WorldObject {
    protected Dictionary<PropertyInt,int?> ephemeralPropertyInts=new();
    public int? EncumbranceVal=5,Value=7,ContainerCapacity;
    public float? UseRadius;
    public bool IsOpen=true;
    public List<WorldObject> Inventory=new();
    public Container(Weenie weenie,ObjectGuid guid):base(weenie.Data){Trace.Add("base:weenie");SetEphemeralValues(false);Populate();}
    public Container(Biota biota):base(biota){Trace.Add("base:biota");SetEphemeralValues(true);Populate();}
    private void Populate(){if(Biota.Existing)Inventory.Add(Item(99));}
    protected WorldObject Item(uint id)=>new(new Biota{Id=id,Spells=new[]{700,701}});
    public void GenerateContainList(){Trace.Add("contain-list");}
    /* CONTAINER_METHODS */
}
public class Corpse:Container { public Corpse(Biota b):base(b){} }
public class DamageHistory {public DamageHistory(Creature c){Trace.Add("damage-history:new");}}
public class CreatureVital {
    private int current;
    public int Current {get=>current;set{current=value;Trace.Add($"vital:{Kind}:{value}");}}
    public PropertyAttribute2nd Kind;
    public int MaxValue=>100+(int)Kind*10;
    public CreatureVital(Creature c,PropertyAttribute2nd kind){Kind=kind;current=c.Biota.Initial;Trace.Add($"vital-init:{kind}:{current}");}
}
public class CreatureAttribute {public CreatureAttribute(Creature c,PropertyAttribute a){Trace.Add($"attribute:{a}");}}
public class CreatureSkill {public CreatureSkill(Creature c,Skill s,PropertiesSkill p){Trace.Add($"skill:{s}");}}
public class Motion { public Motion(MotionStance s,MotionCommand m){Trace.Add("motion:NonCombat:Ready");} }
public class ActionChain {
    public static List<(double Due,Action Action)> Pending=new();
    private double delay;private Action action;
    public void AddDelaySeconds(double seconds){delay=seconds;Trace.Add($"delay:{seconds:R}");}
    public void AddAction(Creature owner,Action value){action=value;Trace.Add("queue:item-spells");}
    public void EnqueueChain(){Pending.Add((delay,action));Trace.Add("enqueue");}
    public static void Advance(double seconds){foreach(var p in Pending.Where(p=>p.Due<=seconds).ToArray()){Pending.Remove(p);p.Action();}}
}
public class Creature:Container {
    public CombatMode CombatMode=CombatMode.Combat;
    public DamageHistory DamageHistory;
    public Dictionary<PropertyAttribute2nd,CreatureVital> Vitals=new();
    public Dictionary<PropertyAttribute,CreatureAttribute> Attributes=new();
    public Dictionary<Skill,CreatureSkill> Skills=new();
    private Dictionary<uint,WorldObjectInfo> selectedTargets;
    public Motion CurrentMotionState;
    public object BiotaDatabaseLock=new();
    public bool Attackable {get=>Biota.Attackable;set=>Biota.Attackable=value;}
    public CreatureVital Health=>Vitals[PropertyAttribute2nd.MaxHealth];
    public CreatureVital Stamina=>Vitals[PropertyAttribute2nd.MaxStamina];
    public CreatureVital Mana=>Vitals[PropertyAttribute2nd.MaxMana];
    public int SelectedCount=>selectedTargets.Count;
    public List<uint> Equipped=new();
    public void GenerateNewFace(){Trace.Add("face");}
    public void GenerateWieldList(){Trace.Add("wield-list");Inventory.Add(Item(101));}
    public void GenerateWieldedTreasure(){Trace.Add("wielded-treasure");Inventory.Add(Item(102));}
    public void GenerateInventoryTreasure(){Trace.Add("inventory-treasure");Inventory.Add(Item(103));}
    public void SetMonsterState(){Trace.Add("monster-state");}
    public List<WorldObject> SelectWieldedTreasure(){Trace.Add("select-all");return Inventory.ToList();}
    public List<WorldObject> SelectWieldedWeapons(){Trace.Add("select-weapons");return Inventory.ToList();}
    public bool TryRemoveFromInventory(ObjectGuid guid){Trace.Add($"remove:{guid.Full}");return Inventory.RemoveAll(i=>i.Guid==guid)>0;}
    public bool TryAddToInventory(WorldObject item){Trace.Add($"restore:{item.Guid.Full}");Inventory.Add(item);return true;}
    public bool TryEquipObject(WorldObject item,EquipMask location){var success=!(Biota.Fail&&item.Guid.Full==101);Trace.Add($"equip:{item.Guid.Full}:{success}");if(success)Equipped.Add(item.Guid.Full);return success;}
    public bool TryWieldObjectWithBroadcasting(WorldObject item,EquipMask location)=>throw new Exception("weapons-only outside constructor path");
    public void CreateItemSpell(WorldObject item,uint spell){Trace.Add($"spell:{item.Guid.Full}:{spell}");}
    /* CREATURE_CS */
    /* CREATURE_EQUIPMENT_CS */
    /* MONSTER_INVENTORY_CS */
}
public class Player:Creature {public Player(Biota b):base(b){}public Player(Weenie w,ObjectGuid g):base(w,g){}}
public static class Program {
    public static void Main(){
        System.Globalization.CultureInfo.CurrentCulture=System.Globalization.CultureInfo.InvariantCulture;
        var cases=new List<object>();
        foreach(var restore in new[]{false,true})foreach(var player in new[]{false,true})
        foreach(var initial in new[]{-5,0,7,200})foreach(var existing in new[]{false,true})
        foreach(var fail in new[]{false,true})foreach(var releaseAttackable in new[]{false,true})
        foreach(var seededDictionaries in new[]{false,true}){
            Trace.Values.Clear();ActionChain.Pending.Clear();
            var biota=new Biota{Initial=initial,Existing=existing,Fail=fail,Attackable=!releaseAttackable,
                PropertiesSkill=seededDictionaries?new(){{Skill.One,new()},{Skill.Two,new()}}:null,
                PropertiesAttribute=seededDictionaries?new():null,PropertiesAttribute2nd=seededDictionaries?new():null,PropertiesBodyPart=seededDictionaries?new():null};
            var weenie=new Weenie{Data=biota};
            Creature value=player?(restore?new Player(biota):new Player(weenie,new(10))):(restore?new Creature(biota):new Creature(weenie,new(10)));
            var constructor=Trace.Values.ToArray();var queued=ActionChain.Pending.Count;
            value.Attackable=releaseAttackable;Trace.Values.Clear();
            ActionChain.Advance(0.099);var beforeDue=Trace.Values.ToArray();
            ActionChain.Advance(0.1);var release=Trace.Values.ToArray();
            cases.Add(new{restore,player,initial,existing,fail,releaseAttackable,seededDictionaries,constructor,queued,beforeDue,release,
                health=value.Health.Current,stamina=value.Stamina.Current,mana=value.Mana.Current,
                equipped=value.Equipped,inventory=value.Inventory.Select(i=>i.Guid.Full).ToArray(),
                combat=value.CombatMode.ToString(),skills=value.Skills.Count,dictionariesPresent=biota.PropertiesAttribute!=null&&biota.PropertiesAttribute2nd!=null&&biota.PropertiesBodyPart!=null&&biota.PropertiesSkill!=null,selected=value.SelectedCount,open=value.IsOpen,capacity=value.ContainerCapacity,useRadius=value.UseRadius});
        }
        Console.WriteLine(JsonSerializer.Serialize(cases));
    }
}
