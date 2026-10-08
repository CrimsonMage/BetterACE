// Unchanged official ACE method and field, AGPL-3.0-only, ACEmulator contributors.
using System;using System.Linq;using System.Collections.Generic;using System.Threading;using ACE.Entity.Enum;
public static class Original{private static HashSet<int> Level8AuraSelfSpells = new HashSet<int>
        {
            (int)SpellId.BloodDrinkerSelf8,
            (int)SpellId.DefenderSelf8,
            (int)SpellId.HeartSeekerSelf8,
            (int)SpellId.SpiritDrinkerSelf8,
            (int)SpellId.SwiftKillerSelf8,
            (int)SpellId.HermeticLinkSelf8,
        };
public static List<PropertiesEnchantmentRegistry> GetEnchantmentsTopLayerByStatModType(this ICollection<PropertiesEnchantmentRegistry> value, EnchantmentTypeFlags statModType, uint statModKey, ReaderWriterLockSlim rwLock, HashSet<int> setSpells, bool handleMultiple = false)
        {
            if (value == null)
                return null;

            rwLock.EnterReadLock();
            try
            {
                var multipleStat = EnchantmentTypeFlags.Undef;

                if (handleMultiple)
                {
                    // todo: this is starting to get a bit messy here, EnchantmentTypeFlags handling should be more adaptable
                    // perhaps the enchantment registry in acclient should be investigated for reference logic

                    multipleStat = statModType | EnchantmentTypeFlags.MultipleStat;

                    statModType |= EnchantmentTypeFlags.SingleStat;
                }

                var valuesByStatModTypeAndKey = value.Where(e => (e.StatModType & statModType) == statModType && e.StatModKey == statModKey || (handleMultiple && (e.StatModType & multipleStat) == multipleStat && (e.StatModType & EnchantmentTypeFlags.Vitae) == 0 && e.StatModKey == 0));

                // 3rd spell id sort added for Gauntlet Damage Boost I / Gauntlet Damage Boost II, which is contained in multiple sets, and can overlap
                // without this sorting criteria, it's already matched up to the client, but produces logically incorrect results for server spell stacking
                // confirmed this bug still exists in acclient Enchantment.Duel(), unknown if it existed in retail server

                var results = from e in valuesByStatModTypeAndKey
                    group e by e.SpellCategory
                    into categories
                    //select categories.OrderByDescending(c => c.LayerId).First();
                    select categories.OrderByDescending(c => c.PowerLevel)
                        .ThenByDescending(c => Level8AuraSelfSpells.Contains(c.SpellId))
                        .ThenByDescending(c => setSpells.Contains(c.SpellId) ? c.SpellId : c.StartTime).First();

                return results.ToList();
            }
            finally
            {
                rwLock.ExitReadLock();
            }
        }}
namespace ACE.Entity.Enum { public enum SpellId {BloodDrinkerSelf8=4395,DefenderSelf8=4400,HeartSeekerSelf8=4405,SpiritDrinkerSelf8=4414,SwiftKillerSelf8=4417,HermeticLinkSelf8=4418}}