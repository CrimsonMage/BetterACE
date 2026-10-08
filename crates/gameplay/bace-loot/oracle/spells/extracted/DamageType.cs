// Verbatim enum extracted from official ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b
namespace ACE.Entity.Enum {
    public enum DamageType
    {
        Undef       = 0x0,
        Slash       = 0x1,
        Pierce      = 0x2,
        Bludgeon    = 0x4,
        Cold        = 0x8,
        Fire        = 0x10,
        Acid        = 0x20,
        Electric    = 0x40,
        Health      = 0x80,
        Stamina     = 0x100,
        Mana        = 0x200,
        Nether      = 0x400,
        Base        = 0x10000000,

        // helpers
        Physical    = Slash | Pierce | Bludgeon,
        Elemental   = Cold | Fire | Acid | Electric,
    }
}
