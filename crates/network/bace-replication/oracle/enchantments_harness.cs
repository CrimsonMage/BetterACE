// Harness only. Official ACE constructors/grouping/writers are compiled unchanged.
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using ACE.Entity.Enum;
using ACE.Entity.Models;
using ACE.Server.Network.Structure;
using ACE.Server.WorldObjects;

public static class Program {
    public static void Main() {
        Write("empty",new());
        var mult=Row(100,3,0.75f);var add=Row(101,7,12.5f);
        var cooldown=Row(0x8001,2,35);cooldown.SpellCategory=(SpellCategory)0x8000;
        cooldown.StatModType=EnchantmentTypeFlags.Cooldown;cooldown.DegradeModifier=.3f;
        cooldown.DegradeLimit=.4f;cooldown.LastTimeDegraded=-2;cooldown.StatModKey=22;
        var vitae=Row(666,1,.95f);vitae.Duration=-1;
        Write("mixed",new(){add,cooldown,vitae,mult});
        Write("cooldown",new(){cooldown});Write("vitae",new(){vitae});
    }
    private static PropertiesEnchantmentRegistry Row(int spell,ushort layer,float value)=>new(){
        SpellId=spell,LayerId=layer,SpellCategory=(SpellCategory)99,PowerLevel=1,
        StartTime=-15,Duration=60,CasterObjectId=0x50000001,HasSpellSetId=false,
        StatModType=EnchantmentTypeFlags.Undef,StatModKey=88,StatModValue=value,
        SpellSetId=(EquipmentSet)42,DegradeModifier=9,DegradeLimit=8,LastTimeDegraded=-7,
    };
    private static void Write(string name,List<PropertiesEnchantmentRegistry> entries){
        var player=new Player();player.Biota.PropertiesEnchantmentRegistry=entries;
        using var stream=new MemoryStream();using var writer=new BinaryWriter(stream);
        writer.Write(new EnchantmentRegistry(player));
        Console.WriteLine(name+","+Convert.ToHexString(stream.ToArray()).ToLowerInvariant());
    }
}
namespace ACE.DatLoader.Entity { public class SpellBase { public uint MetaSpellId; } }
namespace ACE.Entity.Enum {
    public enum SpellId { Vitae=666 }
    public enum SpellCategory { }
    public enum EquipmentSet { }
}
namespace ACE.Entity.Models {
    public static class RegistryClone {
        public static ICollection<PropertiesEnchantmentRegistry> Clone(this ICollection<PropertiesEnchantmentRegistry> entries,object ignored)=>entries.ToList();
    }
}
namespace ACE.Server.WorldObjects {
    public class ObjectGuid { public uint Full=0x50000001; }
    public class WorldObject { public ObjectGuid Guid=new(); public string Name="Synthetic"; }
    public class Biota { public ICollection<PropertiesEnchantmentRegistry> PropertiesEnchantmentRegistry=new List<PropertiesEnchantmentRegistry>(); }
    public class Player:WorldObject {
        public Biota Biota=new();public object BiotaDatabaseLock=new();
        public Managers.EnchantmentManager EnchantmentManager=new();
    }
}
namespace ACE.Server.WorldObjects.Managers {
    public class EnchantmentManager {
        public const ushort SpellCategory_Cooldown=0x8000;
        public void Dispel(PropertiesEnchantmentRegistry entry)=>throw new Exception("Unexpected mutation in projection fixture");
    }
}
namespace ACE.Server.Entity {
    public class Spell {
        public string Name="Synthetic spell";public uint Id,Power=99,StatModKey=7;public SpellCategory Category=(SpellCategory)23;
        public double Duration=120;public float DegradeModifier=.1f,DegradeLimit=.2f,StatModVal;
        public object _spell=new();public bool IsBeneficial=true;public EnchantmentTypeFlags StatModType;
        public Spell(uint id){Id=id;StatModType=id==100?EnchantmentTypeFlags.Multiplicative:EnchantmentTypeFlags.Additive;}
    }
}
