// Official ACEmulator/ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b; AGPL-3.0-only.
using ACE.Entity.Enum;
class Spell { public MagicSchool School; public SpellCategory Category; public bool IsPortalSpell;
public bool HasItemCategory
        {
            get
            {
                switch (Category)
                {
                    case SpellCategory.AttackModRaising:
                    case SpellCategory.DamageRaising:
                    case SpellCategory.DefenseModRaising:
                    case SpellCategory.WeaponTimeRaising:        // verified
                    case SpellCategory.ManaConversionModRaising:
                    case SpellCategory.SpellDamageRaising:
                        return true;
                }
                return false;
            }
        }
}
class Creature {public int Result; void HandleCastSpell(Spell spell, object target, object caster, object? weapon=null,bool equip=false){Result=ReferenceEquals(target,this)?1:2;} public int Route(Spell spell,object item) { Result=0;
switch (spell.School)
            {
                case MagicSchool.CreatureEnchantment:
                case MagicSchool.LifeMagic:

                    HandleCastSpell(spell, this, item, equip: true);
                    break;

                case MagicSchool.ItemEnchantment:

                    if (spell.HasItemCategory || spell.IsPortalSpell)
                        HandleCastSpell(spell, this, item, item, equip: true);
                    else
                        HandleCastSpell(spell, item, item, item, equip: true);

                    break;
            }
return Result;} }
