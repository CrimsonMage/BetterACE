// Original ACE methods, AGPL-3.0-only, ACEmulator contributors. See source.sha256.
using System;using System.Collections.Generic;using ACE.Entity.Enum;
public partial class Player {
public int GetNumItemsDropped(Corpse corpse)
        {
            // Original formula:

            // - When you are level 5 or under, you don't drop anything when you die.
            // - From level 6 to level 10, you lose half your coins (not trade notes) and nothing else.
            // - From level 11 to level 20, you lose half your coins and possibly one non-wielded item (that is, something that you were neither wearing nor holding in your hands).
            // - From level 21 to level 35, you lose half your coins and some number of non-wielded items.
            // - After level 35, you lose half your coins and some number of items. At this point, you can drop items that you were wearing or holding.

            // Now, in those last two cases, I said 'some number'. Some number here is equal to your level divided by 10 (*20 after patch), rounded down, plus a random number between 0 and 2.
            // So from level 21 to 29 you can lose between 2 and 4 items; from level 30 to 39 you can lose 3-5 items; from 40-49 you can lose 4-6 items, and so forth.
            // By level 126 you can be losing up to 14 items.

            // (one caveat here: if you were killed in a PK battle, you always lose items as if you were over level 35, although the exact number you lose
            // is still determined by your real level / 10. In other words, PK deaths do not get the special protection from item loss that NPK deaths get under level 35.)

            // So that's how many items you lose on death -- but how do we determine which items are lost? This is where the categories come into it.

            // take augments into consideration?

            var level = Level ?? 1;

            if (level <= 10)
                return 0;

            if (level >= 11 && level <= 20)
                return ThreadSafeRandom.Next(0, 1);

            // level 21+
            var numItemsDropped = (level / 20) + ThreadSafeRandom.Next(0, 2);

            numItemsDropped = Math.Min(numItemsDropped, MaxItemsDropped);   // is this really a max cap?

            // The number of items you drop can be reduced with the Clutch of the Miser augmentation. If you get the
            // augmentation three times you will no longer drop any items (except half of your Pyreals and all Rares except if you're a PK).
            // If you drop no items, you will not leave a corpse.

            if (!IsPKDeath(corpse.KillerId) && AugmentationLessDeathItemLoss > 0)
            {
                numItemsDropped = Math.Max(0, numItemsDropped - AugmentationLessDeathItemLoss * 5);
            }

            return numItemsDropped;
        }
public int GetNumCoinsDropped()
        {
            // if level > 5, lose half coins
            // (trade notes excluded)
            var level = Level ?? 1;
            var coins = CoinValue ?? 0;

            var numCoinsDropped = level > 5 ? coins / 2 : 0;

            return numCoinsDropped;
        }
public bool IsPKDeath(uint? killerGuid)
        {
            return PlayerKillerStatus.HasFlag(PlayerKillerStatus.PK) && new ObjectGuid(killerGuid ?? 0).IsPlayer() && killerGuid != Guid.Full;
        }
public bool IsPKLiteDeath(uint? killerGuid)
        {
            return PlayerKillerStatus.HasFlag(PlayerKillerStatus.PKLite) && new ObjectGuid(killerGuid ?? 0).IsPlayer() && killerGuid != Guid.Full;
        }
public static HashSet<ushort> NoLog_Landblocks = new HashSet<ushort>()
        {
            // https://asheron.fandom.com/wiki/Special:Search?query=Lifestone+on+Relog%3A+Yes+
            // https://docs.google.com/spreadsheets/d/122xOw3IKCezaTDjC_hggWSVzYJ_9M_zUUtGEXkwNXfs/edit#gid=846612575

            0x0002,     // Viamontian Garrison
            0x0007,     // Town Network
            0x0056,     // Augmentation Realm Main Level
            0x005F,     // Tanada House of Pancakes (Seasonal)
            0x0067,     // PKL Arena
            0x006D,     // Augmentation Realm Upper Level
            0x007D,     // Augmentation Realm Lower Level
            0x00AB,     // Derethian Combat Arena
            0x00AC,     // Derethian Combat Arena
            0x00C3,     // Blighted Putrid Moarsman Tunnels
            0x00D7,     // Jester's Prison
            0x00EA,     // Mhoire Armory
            0x015D,     // Mountain Cavern
            0x027F,     // East Fork Dam Hive
            0x03A7,     // Mount Elyrii Hive
            0x5764,     // Oubliette of Mhoire Castle
            0x634C,     // Tainted Grotto
            0x6544,     // Greater Battle Dungeon
            0x6651,     // Hoshino Tower
            0x7E04,     // Thug Hideout
            0x8A04,     // Night Club (Seasonal Anniversary)
            0x8B04,     // Frozen Wight Lair
            0x9EE5,     // Northwatch Castle Black Market
            0xB5F0,     // Aerfalle's Sanctum
            0xF92F,     // Freebooter Keep Black Market
            0x00B0,     // Colosseum Arena One
            0x00B1,     // Colosseum Arena Two
            0x00B2,     // Colosseum Arena Three
            0x00B3,     // Colosseum Arena Four
            0x00B4,     // Colosseum Arena Five
            0x00B6,     // Colosseum Arena Mini-Bosses
            0x5960,     // Gauntlet Arena One (Celestial Hand)
            0x5961,     // Gauntlet Arena Two (Celestial Hand)
            0x5962,     // Gauntlet Arena One (Eldritch Web)
            0x5963,     // Gauntlet Arena Two (Eldritch Web)
            0x5964,     // Gauntlet Arena One (Radiant Blood)
            0x5965,     // Gauntlet Arena Two (Radiant Blood)
        };
}
public partial class Corpse {
public static HashSet<ushort> NoDrop_Landblocks = new HashSet<ushort>()
        {
            0x005F,     // Tanada House of Pancakes (Seasonal)
            0x00AF,     // Colosseum Staging Area and Secret Mini-Bosses
            0x00B0,     // Colosseum Arena One
            0x00B1,     // Colosseum Arena Two
            0x00B2,     // Colosseum Arena Three
            0x00B3,     // Colosseum Arena Four
            0x00B4,     // Colosseum Arena Five
            0x00B6,     // Colosseum Arena Mini-Bosses
            0x00EA,     // Mhoire Armory
            0x33F4,     // Frozen Cave
            0x5960,     // Gauntlet Arena One (Celestial Hand)
            0x5961,     // Gauntlet Arena Two (Celestial Hand)
            0x5962,     // Gauntlet Arena One (Eldritch Web)
            0x5963,     // Gauntlet Arena Two (Eldritch Web)
            0x5964,     // Gauntlet Arena One (Radiant Blood)
            0x5965,     // Gauntlet Arena Two (Radiant Blood)
            0x596B,     // Gauntlet Staging Area (All Societies)
            0x8A04,     // Night Club (Seasonal Anniversary)
            0xB5F0,     // Aerfalle's Sanctum
        };
}
public class Creature {
public static HashSet<ushort> NoDeathXP_Landblocks = new HashSet<ushort>()
        {
            0x00B0,     // Colosseum Arena One
            0x00B1,     // Colosseum Arena Two
            0x00B2,     // Colosseum Arena Three
            0x00B3,     // Colosseum Arena Four
            0x00B4,     // Colosseum Arena Five
            0x5960,     // Gauntlet Arena One (Celestial Hand)
            0x5961,     // Gauntlet Arena Two (Celestial Hand)
            0x5962,     // Gauntlet Arena One (Eldritch Web)
            0x5963,     // Gauntlet Arena Two (Eldritch Web)
            0x5964,     // Gauntlet Arena One (Radiant Blood)
            0x5965,     // Gauntlet Arena Two (Radiant Blood)
            0x596B,     // Gauntlet Staging Area (All Societies)
        };
}
public class EnchantmentManager {
public float GetMinVitae(uint level)
        {
            var propVitae = 1.0 - PropertyManager.GetDouble("vitae_penalty_max").Item;

            var maxPenalty = (level - 1) * 3;
            if (maxPenalty < 1)
                maxPenalty = 1;

            var globalMax = 100 - (uint)Math.Round(propVitae * 100);
            if (maxPenalty > globalMax)
                maxPenalty = globalMax;

            var minVitae = (100 - maxPenalty) / 100.0f;
            if (minVitae < propVitae)
                minVitae = (float)propVitae;

            return minVitae;
        }
}
public partial class Position {
public float SquaredDistanceTo(Position p)
        {
            if (p == null) return float.MaxValue;

            if (p.LandblockId == this.LandblockId)
            {
                var dx = this.PositionX - p.PositionX;
                var dy = this.PositionY - p.PositionY;
                var dz = this.PositionZ - p.PositionZ;
                return dx * dx + dy * dy + dz * dz;
            }
            //if (p.LandblockId.MapScope == MapScope.Outdoors && this.LandblockId.MapScope == MapScope.Outdoors)
            else
            {
                // verify this is working correctly if one of these is indoors
                var dx = (this.LandblockId.LandblockX - p.LandblockId.LandblockX) * 192 + this.PositionX - p.PositionX;
                var dy = (this.LandblockId.LandblockY - p.LandblockId.LandblockY) * 192 + this.PositionY - p.PositionY;
                var dz = this.PositionZ - p.PositionZ;
                return dx * dx + dy * dy + dz * dz;
            }
        }
}
