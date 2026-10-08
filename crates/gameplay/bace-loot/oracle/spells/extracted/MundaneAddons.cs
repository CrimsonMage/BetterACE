// Unchanged official ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b methods; AGPL-3.0-only.
using ACE.Common;using ACE.Database.Models.World;using ACE.Server.WorldObjects;using ACE.Server.Managers;namespace ACE.Server.Factories {public static partial class LootGenerationFactory {
        private static WorldObject TryRollMundaneAddon(TreasureDeath profile)
        {
            // coalesced mana only dropped in tiers 1-4
            if (profile.Tier <= 4)
                return TryRollCoalescedMana(profile);

            // aetheria dropped in tiers 5+
            else
                return TryRollAetheria(profile);
        }
        private static WorldObject TryRollCoalescedMana(TreasureDeath profile)
        {
            // 2% chance in here, which turns out to be less per corpse w/ MundaneItemChance > 0,
            // when the outer MundaneItemChance roll is factored in

            // loot quality mod?
            var rng = ThreadSafeRandom.Next(0.0f, 1.0f);

            if (rng < 0.02f)
                return CreateCoalescedMana(profile);
            else
                return null;
        }
        private static WorldObject TryRollAetheria(TreasureDeath profile)
        {
            var aetheria_drop_rate = (float)PropertyManager.GetDouble("aetheria_drop_rate").Item;

            if (aetheria_drop_rate <= 0.0f)
                return null;

            var dropRateMod = 1.0f / aetheria_drop_rate;

            // 2% base chance in here, which turns out to be less per corpse w/ MundaneItemChance > 0,
            // when the outer MundaneItemChance roll is factored in

            // loot quality mod?
            var rng = ThreadSafeRandom.Next(0.0f, 1.0f * dropRateMod);

            if (rng < 0.02f)
                return CreateAetheria(profile);
            else
                return null;
        }
}}
