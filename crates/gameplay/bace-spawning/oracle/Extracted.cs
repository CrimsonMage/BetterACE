// ACEmulator/ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b; AGPL-3.0-only. Extracted unchanged methods.
// Source Source/ACE.Server/WorldObjects/WorldObject_Generators.cs SHA256 54d8b951e9714730ecd2e2dddcfc933e7a0a05485d4b06fc858db12d0f27bff3
using System;
public partial class WorldObject {
        public void SelectAProfile()
        {
            //History.Add($"[{DateTime.UtcNow}] - SelectAProfile()");

            //bool rng_selected = false;

            if (GenStopSelectProfileConditions)
                return;

            //var totalProbability = rng_selected ? GetTotalProbability() : 1.0f;
            //var rng = ThreadSafeRandom.Next(0.0f, totalProbability);
            //var rng = ThreadSafeRandom.Next(0.0f, 1.0f);
            var rng = ThreadSafeRandom.Next(0.0f, GetTotalProbability());

            for (var i = 0; i < GeneratorProfiles.Count; i++)
            {
                var profile = GeneratorProfiles[i];

                // skip PlaceHolder objects
                if (profile.IsPlaceholder)
                    continue;

                // is this profile already at its max_create?
                if (profile.IsMaxed)
                    continue;

                // is this profile currently timed out?
                if (!profile.IsAvailable)
                    continue;

                if (profile.RegenLocationType.HasFlag(RegenLocationType.Treasure))
                {
                    if (profile.Biota.InitCreate > 1)
                    {
                        log.Warn($"[GENERATOR] 0x{Guid} {Name}.SelectAProfile(): profile[{i}].RegenLocationType({profile.RegenLocationType}), profile.Biota.WCID({profile.Biota.WeenieClassId}), profile.Biota.InitCreate({profile.Biota.InitCreate}) > 1, set to 1. WCID: {WeenieClassId} - LOC: {Location.ToLOCString()}");
                        profile.Biota.InitCreate = 1;
                    }

                    if (profile.Biota.MaxCreate > 1)
                    {
                        log.Warn($"[GENERATOR] 0x{Guid} {Name}.SelectAProfile(): profile[{i}].RegenLocationType({profile.RegenLocationType}), profile.Biota.WCID({profile.Biota.WeenieClassId}), profile.Biota.MaxCreate({profile.Biota.MaxCreate}) > 1, set to 1. WCID: {WeenieClassId} - LOC: {Location.ToLOCString()}");
                        profile.Biota.MaxCreate = 1;
                    }
                }

                //var probability = rng_selected ? GetAdjustedProbability(i) : profile.Biota.Probability;
                //var probability = profile.Biota.Probability;
                var probability = GetAdjustedProbability(i);

                if (rng < probability || probability == -1)
                {
                    var numObjects = GetSpawnObjectsForProfile(profile);
                    profile.Enqueue(numObjects);
                    //log.Info($"[GENERATOR] 0x{Guid} {Name}.SelectAProfile(): profile[{i}] Enqueued {numObjects} {profile.Biota.WeenieClassId} for spawning. MaxObjectsSpawned = {profile.MaxObjectsSpawned} | Exhusted = {profile.RemoveQueue.Count == profile.MaxCreate} | {profile.CurrentCreate} | {profile.MaxCreate} | {profile.Spawned.Count} | {profile.RemoveQueue.Count}");

                    //var rng_str = probability == -1 ? "" : "RNG ";
                    //History.Add($"[{DateTime.UtcNow}] - SelectAProfile() - {rng_str}selected slot {i} to spawn, adding {numObjects} objects ({profile.CurrentCreate}/{profile.MaxCreate})");

                    // if RNG rolled, we are done with this roll
                    if (profile.Biota.Probability != -1)
                    {
                        //rng_selected = true;
                        break;
                    }

                    // stop conditions
                    if (GenStopSelectProfileConditions)
                        return;
                }
            }
        }
        public float GetTotalProbability()
        {
            var totalProbability = 0.0f;
            var lastProbability = 0.0f;

            foreach (var profile in GeneratorProfiles)
            {
                var probability = profile.Biota.Probability;

                if (probability == -1)
                {
                    //if (!profile.IsMaxed)
                    if (!profile.IsMaxed && profile.IsAvailable)
                        return 1.0f;

                    continue;
                }
                //if (!profile.IsMaxed)
                if (!profile.IsMaxed && profile.IsAvailable)
                {
                    if (lastProbability > probability)
                        lastProbability = 0.0f;

                    var diff = probability - lastProbability;
                    totalProbability += diff;
                }
                lastProbability = probability;
            }

            return totalProbability;
        }
        public float GetAdjustedProbability(int index)
        {
            // say theres a generator with 2 profiles
            // the first has a 99% chance to spawn, and the second has a 1% chance
            // the generator init_create is 1, and the max_create is 2
            // when the generator first spawns in, the first object is created, as expected
            // then the hearbeat happens later, and it sees it can spawn up to 1 additional object
            // the rare item is the only item left, with the 1 % chance
            // so the question is, in that scenario, would the rare item always spawn then?
            // or would it only do 1 roll, and the rare item would still have only a 1% chance to spawn?

            var profile = GeneratorProfiles[index];
            if (profile.Biota.Probability == -1)
                return -1;

            var totalProbability = 0.0f;
            var lastProbability = 0.0f;

            for (var i = 0; i <= index; i++)
            {
                profile = GeneratorProfiles[i];
                var probability = profile.Biota.Probability;

                if (probability == -1)
                    continue;

                //if (!profile.IsMaxed)
                if (!profile.IsMaxed && profile.IsAvailable)
                {
                    if (lastProbability > probability)
                        lastProbability = 0.0f;

                    var diff = probability - lastProbability;
                    totalProbability += diff;
                }
                lastProbability = probability;
            }
            return totalProbability;
        }
        public int GetSpawnObjectsForProfile(GeneratorProfile profile)
        {
            // get the number of objects to spawn for this profile
            // usually profile.InitCreate, must be at least profile.InitCreate while not to exceed generator.MaxCreate and profile.MaxCreate,
            // -1 for profile.InitCreate == 1
            // -1 for profile.MaxCreate == profile can be spawned infinitely as long as generator.MaxCreate has not been met.

            var initCreate = profile.InitCreate;
            var maxCreate = profile.MaxCreate;
            var numObjects = 0;

            if (initCreate == -1 || maxCreate == -1)
                numObjects = 1;
            else
                numObjects = initCreate;

            //Console.WriteLine($"INIT - 0x{Guid} {Name} ({WeenieClassId}): CurrentCreate = {CurrentCreate} | profile.Biota.InitCreate = {profile.Biota.InitCreate} | profile.Biota.MaxCreate = {profile.Biota.MaxCreate} | InitCreate: {InitCreate} | MaxCreate: {MaxCreate} | initCreate: {initCreate} | maxCreate: {maxCreate} | leftObjects = {leftObjects} | numObjects: {numObjects}");            

            var genSlotsAvailable = MaxCreate - CurrentCreate;
            var profileSlotsAvailable = profile.MaxCreate - profile.CurrentCreate;

            if (genSlotsAvailable < numObjects)
                numObjects = genSlotsAvailable;

            if (profile.MaxCreate != -1 && profileSlotsAvailable < numObjects)
                numObjects = profileSlotsAvailable;

            if (numObjects == 0 && initCreate == 0)
                log.Warn($"[GENERATOR] 0x{Guid}:{WeenieClassId} {Name}.GetSpawnObjectsForProfile(profile[{profile.LinkId}]): profile.InitCreate = {profile.InitCreate} | profile.MaxCreate = {profile.MaxCreate} | profile.WeenieClassId = {profile.WeenieClassId} | Profile Init invalid, cannot spawn.");
            else if (numObjects == 0)
               log.Warn($"[GENERATOR] 0x{Guid}:{WeenieClassId} {Name}.GetSpawnObjectsForProfile(profile[{profile.LinkId}]): profile.InitCreate = {profile.InitCreate} | profile.MaxCreate = {profile.MaxCreate} | profile.WeenieClassId = {profile.WeenieClassId} | genSlotsAvailable = {genSlotsAvailable} | profileSlotsAvailable = {profileSlotsAvailable} | numObjects = {numObjects}, cannot spawn.");

            return numObjects;
        }
        public bool GenStopSelectProfileConditions
        {
            get
            {
                if (CurrentCreate >= MaxCreate)
                {
                    //if (CurrentCreate > InitCreate)
                    //log.DebugFormat("{0} - 0x{1}:{2}.StopConditionsInit(): CurrentCreate({3}) > InitCreate({4})", WeenieClassId, Guid, Name, CurrentCreate, InitCreate);

                    return true;
                }

                if (CurrentlyPoweringUp && CurrentCreate >= InitCreate)
                {
                    return true;
                }

                return AllProfilesUnavailable || AllProfilesMaxed;
            }
        }
}
