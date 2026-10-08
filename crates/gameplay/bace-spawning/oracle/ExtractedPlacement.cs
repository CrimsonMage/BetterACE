// ACEmulator/ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b; AGPL-3.0-only. Extracted unchanged methods.
// Source Source/ACE.Server/Entity/GeneratorProfile.cs SHA256 52b6eefed38d24ac107fed7bf44bf42a9557af52a6319ae8ccb7e6302abece23
using System;
using System.Numerics;
public partial class GeneratorProfile {
        public bool Spawn_Specific(WorldObject obj)
        {
            // specific position
            if ((Biota.ObjCellId ?? 0) > 0)
                obj.Location = new ACE.Entity.Position(Biota.ObjCellId ?? 0, Biota.OriginX ?? 0, Biota.OriginY ?? 0, Biota.OriginZ ?? 0, Biota.AnglesX ?? 0, Biota.AnglesY ?? 0, Biota.AnglesZ ?? 0, Biota.AnglesW ?? 0);

            // offset from generator location
            else
            {
                if (PropertyManager.GetBool("use_generator_rotation_offset").Item)
                {
                    var offset = Vector3.Transform(new Vector3(Biota.OriginX ?? 0, Biota.OriginY ?? 0, Biota.OriginZ ?? 0), Generator.Location.Rotation);

                    if (Generator.GeneratorType == GeneratorType.Relative)
                    {
                        var rotate = new Quaternion(Biota.AnglesX ?? 0, Biota.AnglesY ?? 0, Biota.AnglesZ ?? 0, Biota.AnglesW ?? 0) * Generator.Location.Rotation;

                        obj.Location = new ACE.Entity.Position(Generator.Location.Cell, Generator.Location.PositionX + offset.X, Generator.Location.PositionY + offset.Y, Generator.Location.PositionZ + offset.Z, rotate.X, rotate.Y, rotate.Z, rotate.W);
                    }
                    else
                        obj.Location = new ACE.Entity.Position(Generator.Location.Cell, Generator.Location.PositionX + offset.X, Generator.Location.PositionY + offset.Y, Generator.Location.PositionZ + offset.Z, Biota.AnglesX ?? 0, Biota.AnglesY ?? 0, Biota.AnglesZ ?? 0, Biota.AnglesW ?? 0);
                }
                else
                    obj.Location = new ACE.Entity.Position(Generator.Location.Cell, Generator.Location.PositionX + Biota.OriginX ?? 0, Generator.Location.PositionY + Biota.OriginY ?? 0, Generator.Location.PositionZ + Biota.OriginZ ?? 0, Biota.AnglesX ?? 0, Biota.AnglesY ?? 0, Biota.AnglesZ ?? 0, Biota.AnglesW ?? 0);
            }

            if (!VerifyLandblock(obj) || !VerifyWalkableSlope(obj))
                return false;

            return obj.EnterWorld();
        }
}
