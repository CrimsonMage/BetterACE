using System;
using System.Collections.Generic;
using System.Linq;
using System.IO;
using ACE.Server.Network.Structure;
using System.Text;
using System.Text.Json;
using ACE.Entity;
using ACE.Entity.Enum;
using ACE.DatLoader;
using ACE.Entity.Enum.Properties;
using ACE.Server.WorldObjects;
using ACE.Server.WorldObjects.Entity;
using ACE.Server.Network;
using ACE.Server.Network.Enum;
using ACE.Server.Network.GameMessages;
using ACE.Server.Network.GameMessages.Messages;
using ACE.Server.Network.GameEvent;
using ACE.Server.Network.GameEvent.Events;
using ACE.Server.Network.GameAction;
using ACE.Server.Network.GameAction.Actions;
using ACE.Common.Extensions;

class Program
{
    static Dictionary<string, uint> Catalog<T>() where T : struct, System.Enum =>
        System.Enum.GetNames<T>().ToDictionary(name => name, name => Convert.ToUInt32(System.Enum.Parse<T>(name)));
    static void ClientString(BinaryWriter writer, string value) {
        writer.Write((ushort)value.Length); writer.Write(Encoding.UTF8.GetBytes(value));
        writer.Write(new byte[(4-(value.Length+2)%4)%4]);
    }
    static void Main()
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        var messages = new Dictionary<string, object>();
        void Add(string name, GameMessage msg) => messages.Add(name, new { bytes = Convert.ToHexString(msg.Data.ToArray()), group = (uint)msg.Group });
        foreach (var error in System.Enum.GetValues<CharacterError>()) Add("error_" + error, new GameMessageCharacterError(error));
        Add("created", new GameMessageCharacterCreateResponse(CharacterGenerationVerificationResponse.Ok, new ObjectGuid(0x50000001), "Élodie"));
        Add("create_failed", new GameMessageCharacterCreateResponse(CharacterGenerationVerificationResponse.NameInUse, new ObjectGuid(0), null));
        Add("restored", new GameMessageCharacterRestore(0x50000002, "Restored", 3600));
        Add("deleted", new GameMessageCharacterDelete());
        Add("logged_off", new GameMessageCharacterLogOff());
        Add("world_ready", new GameMessageCharacterEnterWorldServerReady());
        var session = new Session { Account = "synthetic-account", AccessLevel = AccessLevel.Player, Player = new HarnessPlayer { Guid = new ObjectGuid(0x50000001) }, GameEventSequence = 42 };
        Add("character_list", new GameMessageCharacterList(new List<ACE.Database.Models.Shard.Character> {
            new() { Id = 0x50000001, Name = "Élodie" }, new() { Id = 0x50000002, Name = "Former", DeleteTime = 90 }
        }, session));
        Add("empty_character_list", new GameMessageCharacterList(new(), session));
        Add("server_name", new GameMessageServerName("BACE €", 3, -1));
        Add("system_chat", new GameMessageSystemChat("Hello €", (ChatMessageType)7));
        Add("speech", new GameMessageHearSpeech("Hello €", "Élodie", 0x50000001, (ChatMessageType)2));
        Add("ranged_speech", new GameMessageHearRangedSpeech("Hello €", "Élodie", 0x50000001, 12.5f, (ChatMessageType)2));
        Add("emote", new GameMessageEmoteText(0x50000001, "Élodie", "waves"));
        Add("soul_emote", new GameMessageSoulEmote(0x50000001, "Élodie", "waves"));
        Add("boot_null", new GameMessageBootAccount());
        Add("boot_empty", new GameMessageBootAccount(""));
        Add("boot_reason", new GameMessageBootAccount(" because maintenance"));
        foreach (var entry in new[] { ("ban_null", (string)null), ("ban_empty", ""), ("ban_reason", " because synthetic policy") }) {
            // The upstream constructor reads UTC internally. A half-second margin
            // produces a known integral duration; fail rather than record timing drift.
            var ban = new GameMessageAccountBanned(DateTime.UtcNow.AddSeconds(3600.5), entry.Item2);
            if (BitConverter.ToUInt32(ban.Data.ToArray(), 4) != 3600) throw new Exception("Ban oracle exceeded clock margin; regenerate");
            Add(entry.Item1, ban);
        }
        Add("ddd_interrogation", new GameMessageDDDInterrogation());
        ACE.Server.Managers.PropertyManager.HighRes = true;
        Add("ddd_interrogation_highres", new GameMessageDDDInterrogation());
        Add("ddd_end", new GameMessageDDDEndDDD());
        Add("ddd_error", new GameMessageDDDErrorMessage(7, 0x12345678, 1));
        var iterations = new Dictionary<DatDatabaseType, Dictionary<uint, List<uint>>> {
            [DatDatabaseType.Cell] = new() { [8] = new() { 0x1234FFFE, 0x1234FFFF } },
            [DatDatabaseType.HighRes] = new() { [9] = new() { 0x06000001 } },
            [DatDatabaseType.Language] = new() { [7] = new() { 0x25000001 } },
            [DatDatabaseType.Portal] = new() { [6] = new() { 0x01000001, 0x02000002 }, [5] = new() { 0x03000003 } },
        };
        Add("ddd_begin", new GameMessageDDDBeginDDD(5, 12345, iterations));
        foreach (var database in new[] { DatDatabaseType.Portal, DatDatabaseType.Cell, DatDatabaseType.Language, DatDatabaseType.HighRes }) {
            ACE.Server.Managers.DDDManager.Compressed = false;
            Add("ddd_data_" + database, new GameMessageDDDDataMessage(0x12345678, database));
        }
        ACE.Server.Managers.DDDManager.Compressed = true;
        Add("ddd_data_compressed", new GameMessageDDDDataMessage(0x12345678, DatDatabaseType.Portal));
        Add("fellow_update_done", new GameEventFellowshipFellowUpdateDone(session, (WeenieError)123));
        Add("weenie_error", new GameEventWeenieError(session, (WeenieError)0x1234));
        Add("use_done", new GameEventUseDone(session, (WeenieError)0x36));
        Add("ping_response", new GameEventPingResponse(session));
        session.Player = null;
        Add("ping_without_player", new GameEventPingResponse(session));
        var worldObject = new WorldObject();
        Add("private_int", new GameMessagePrivateUpdatePropertyInt(worldObject, (PropertyInt)42, -1234));
        Add("public_int", new GameMessagePublicUpdatePropertyInt(worldObject, (PropertyInt)42, -1234));
        Add("private_int64", new GameMessagePrivateUpdatePropertyInt64(worldObject, (PropertyInt64)42, -1234567890123L));
        Add("public_int64", new GameMessagePublicUpdatePropertyInt64(worldObject, (PropertyInt64)42, -1234567890123L));
        Add("private_bool", new GameMessagePrivateUpdatePropertyBool(worldObject, (PropertyBool)42, true));
        Add("public_bool", new GameMessagePublicUpdatePropertyBool(worldObject, (PropertyBool)42, true));
        Add("private_float", new GameMessagePrivateUpdatePropertyFloat(worldObject, (PropertyFloat)42, -12.25));
        Add("public_float", new GameMessagePublicUpdatePropertyFloat(worldObject, (PropertyFloat)42, -12.25));
        Add("private_string", new GameMessagePrivateUpdatePropertyString(worldObject, (PropertyString)42, "Café €"));
        Add("public_string", new GameMessagePublicUpdatePropertyString(worldObject, (PropertyString)42, "Café €"));
        Add("private_dataid", new GameMessagePrivateUpdateDataID(worldObject, (PropertyDataId)42, 0x06001234u));
        Add("public_dataid", new GameMessagePublicUpdatePropertyDataID(worldObject, (PropertyDataId)42, 0x06001234u));
        Add("private_instanceid", new GameMessagePrivateUpdateInstanceID(worldObject, (PropertyInstanceId)42, 0x50000002u));
        Add("public_instanceid", new GameMessagePublicUpdateInstanceID(worldObject, (PropertyInstanceId)42, new ObjectGuid(0x50000002)));
        Add("attribute", new GameMessagePrivateUpdateAttribute(worldObject, new CreatureAttribute()));
        Add("private_vital", new GameMessagePrivateUpdateVital(worldObject, new CreatureVital()));
        Add("public_vital", new GameMessagePublicUpdateVital(worldObject, (PropertyAttribute2nd)2, 3, 4, 5, 6));
        Add("skill", new GameMessagePrivateUpdateSkill(worldObject, new CreatureSkill()));
        Add("current_vital", new GameMessagePrivateUpdateAttribute2ndLevel(worldObject, (Vital)2, 6));
        session.Player = new HarnessPlayer();
        var progressionRequests = new[] { GameActionType.RaiseAttribute, GameActionType.RaiseVital, GameActionType.RaiseSkill, GameActionType.TrainSkill }.SelectMany(action => new[] { false, true }.Select(trailing => {
            using var ms = new MemoryStream(); using var writer = new BinaryWriter(ms);
            writer.Write((uint)GameMessageOpcode.GameAction); writer.Write(42u); writer.Write((uint)action);
            writer.Write(2u); writer.Write(action == GameActionType.TrainSkill ? unchecked((uint)-3) : uint.MaxValue);
            if (trailing) writer.Write(new byte[] { 0xa5, 0x5a });
            var bytes = ms.ToArray(); var message = new ClientMessage(bytes);
            message.Payload.ReadUInt32(); message.Payload.ReadUInt32();
            switch (action) {
                case GameActionType.RaiseAttribute: GameActionRaiseAttribute.Handle(message, session); break;
                case GameActionType.RaiseVital: GameActionRaiseVital.Handle(message, session); break;
                case GameActionType.RaiseSkill: GameActionRaiseSkill.Handle(message, session); break;
                case GameActionType.TrainSkill: GameActionTrainSkill.Handle(message, session); break;
            }
            return new { bytes = Convert.ToHexString(bytes), target = session.Player.LastTarget, amount = session.Player.LastAmount, consumed = message.Data.Position, trailing_bytes = message.Data.Length-message.Data.Position };
        })).ToArray();
        var creationRequests = new[] { "Synthetic", "Élodie", "astral🙂" }.Select((name, index) => {
            using var ms = new MemoryStream(); using var writer = new BinaryWriter(ms);
            writer.Write((uint)GameMessageOpcode.CharacterCreate); ClientString(writer, "synthetic-account");
            writer.Write(99u); writer.Write(2u); writer.Write(1u);
            for (uint i = 1; i <= 14; i++) writer.Write(i);
            for (uint i = 1; i <= 6; i++) writer.Write(i / 10.0);
            writer.Write(-2); for (uint i = 1; i <= 6; i++) writer.Write(i * 10);
            writer.Write(3u); writer.Write(4u);
            writer.Write(index == 0 ? 0u : 4u);
            if (index != 0) foreach (uint skill in new uint[] { 0, 1, 2, 3 }) writer.Write(skill);
            ClientString(writer, name); writer.Write(5u); writer.Write(1u); writer.Write(2u);
            if (index == 2) writer.Write(new byte[] { 0xa5, 0x5a });
            var bytes = ms.ToArray(); var message = new ClientMessage(bytes);
            var account = message.Payload.ReadString16L();
            var info = new CharacterCreateInfo(); info.Unpack(message.Payload);
            return new { bytes = Convert.ToHexString(bytes), account, decoded = info, skills = info.SkillAdvancementClasses, consumed = message.Data.Position, trailing_bytes = message.Data.Length-message.Data.Position };
        }).ToArray();
        var actions = new[] { (0u, GameActionType.LoginComplete, Array.Empty<byte>()), (42u, GameActionType.Use, new byte[] { 1, 2, 3, 4 }), (uint.MaxValue, GameActionType.Talk, new byte[] { 1, 0, 65, 0 }), (8u, unchecked((GameActionType)uint.MaxValue), new byte[] { 0xff }) }.Select(input => {
            using var ms = new MemoryStream(); using var writer = new BinaryWriter(ms);
            writer.Write((uint)GameMessageOpcode.GameAction); writer.Write(input.Item1); writer.Write((uint)input.Item2); writer.Write(input.Item3);
            var bytes = ms.ToArray(); var message = new ClientMessage(bytes);
            GameActionPacket.HandleGameAction(message, session);
            return new { bytes = Convert.ToHexString(bytes), sequence = input.Item1, action = (uint)ACE.Server.Network.Managers.InboundMessageManager.LastAction, payload = Convert.ToHexString(ACE.Server.Network.Managers.InboundMessageManager.LastPayload), consumed = ACE.Server.Network.Managers.InboundMessageManager.Consumed, opcode = message.Opcode };
        }).ToArray();
        var iterationSets = new[] { new int[] { 0 }, new int[] { 5, 1, -5 }, new int[] { 3, 10, 11, 12 }, new int[] { 2, -1, 4, 5 } }.Select(words => {
            using var ms = new MemoryStream(); using var writer = new BinaryWriter(ms);
            writer.Write(1); writer.Write(0); writer.Write(1);
            foreach (var word in words) writer.Write(word);
            var bytes = ms.ToArray(); ms.Position = 0;
            var decoded = new BinaryReader(ms).ReadCAllIterationList().Lists[0];
            return new { bytes = Convert.ToHexString(bytes), type = decoded.DatFileType, id = decoded.DatFileId, iterations = decoded.List.Iterations, runs = decoded.List.Ints };
        }).ToArray();
        Console.WriteLine(JsonSerializer.Serialize(new {
            catalogs = new { messages = Catalog<GameMessageOpcode>(), actions = Catalog<GameActionType>(), events = Catalog<GameEventType>(), character_errors = Catalog<CharacterError>(), groups = Catalog<GameMessageGroup>() },
            messages, iteration_sets = iterationSets, actions, progression_requests = progressionRequests, creation_requests = creationRequests, movement = MovementHarness.Run(), objects = ObjectHarness.Run(), social = SocialHarness.Run(), inventory = InventoryHarness.Run(),
        }));
    }
}
// Harness-only domain adapters. All wire bytes are written by unchanged official
// serializers above. These inputs avoid live accounts, clocks, DAT files and DBs.
namespace ACE.Server.Network {
    public sealed partial class HarnessPlayer : Player {
        public uint LastTarget; public long LastAmount;
        public bool LastContact, Teleporting, IsPlayerMovingTo, IsPlayerMovingTo2;
        public Position LastGroundPos, RequestedLocation;
        public ACE.Server.Network.Structure.JumpPack LastJump;
        public void SetRequestedLocation(Position position) { RequestedLocation = position; }
        public void HandleActionJump(ACE.Server.Network.Structure.JumpPack pack) { LastJump = pack; }
        public void StopExistingMoveToChains() { }
        public void StopExistingMoveToChains2() { }

        public void HandleActionRaiseAttribute(PropertyAttribute attribute, uint amount) { LastTarget = (uint)attribute; LastAmount = amount; }
        public void HandleActionRaiseVital(PropertyAttribute2nd vital, uint amount) { LastTarget = (uint)vital; LastAmount = amount; }
        public void HandleActionRaiseSkill(Skill skill, uint amount) { LastTarget = (uint)skill; LastAmount = amount; }
        public void HandleActionTrainSkill(Skill skill, int amount) { LastTarget = (uint)skill; LastAmount = amount; }
    }
    public sealed class Session { public string Account; public AccessLevel AccessLevel; public HarnessPlayer Player; public uint GameEventSequence; }
}
namespace ACE.Database.Models.Shard {
    public sealed class Character { public uint Id; public string Name; public bool IsPlussed; public ulong DeleteTime; }
}
namespace ACE.Common {
    public static class Time { public static double GetUnixTime() => 100; }
    public static class ConfigManager { public static HarnessConfig Config = new(); }
    public class HarnessConfig { public HarnessServer Server = new(); }
    public class HarnessServer { public HarnessAccounts Accounts = new(); }
    public class HarnessAccounts { public bool OverrideCharacterPermissions; }
}
namespace ACE.DatLoader {
    public sealed class HarnessFile { public uint ObjectId = 0x12345678; public uint Iteration = 19; public uint GetFileType(DatDatabaseType _) => 7; }
}
namespace ACE.Server.Managers {
    public sealed class Property<T> { public T Item; }
    public static class PropertyManager {
        public static bool HighRes;
        public static Property<long> GetLong(string name) => new() { Item = 11 };
        public static Property<bool> GetBool(string name) => new() { Item = name == "allow_highres_dat" ? HighRes : true };
    }
    public static class DDDManager {
        public static readonly int HiFi_String_As_Int = BitConverter.ToInt32(Encoding.UTF8.GetBytes("HiFi"), 0);
        public static bool Compressed;
        public static byte[] TryGetDatFileContentsForTransmission(uint id, DatDatabaseType database, out HarnessFile file, out bool compressed) {
            file = new(); compressed = Compressed; return new byte[] { 1, 2, 3, 4, 5 };
        }
    }
}

namespace ACE.Entity.Enum.Properties {
    public enum PropertyInt : ushort { StackSize = 12 }
    public enum PropertyInt64 : ushort { }
    public enum PropertyBool : ushort { }
    public enum PropertyFloat : ushort { }
    public enum PropertyString : ushort { }
    public enum PropertyDataId : ushort { }
    public enum PropertyInstanceId : ushort { }
    public enum PropertyAttribute : ushort { }
    public enum PropertyAttribute2nd : ushort { }
}
namespace ACE.Entity.Enum {
    public enum Vital : ushort { }
    public enum Skill { }
    public enum SkillAdvancementClass : uint { }
}
namespace ACE.Server.WorldObjects {
    public sealed class HarnessSequences {
        public byte[] GetCurrentSequence(ACE.Server.Network.Sequence.SequenceType type) => BitConverter.GetBytes((ushort)(type switch {
            ACE.Server.Network.Sequence.SequenceType.ObjectInstance => 0x1122,
            ACE.Server.Network.Sequence.SequenceType.ObjectTeleport => 0x5566,
            ACE.Server.Network.Sequence.SequenceType.ObjectForcePosition => 0x7788,
            ACE.Server.Network.Sequence.SequenceType.ObjectServerControl => 0x2345,
            _ => 0,
        }));
        public byte[] GetNextSequence(ACE.Server.Network.Sequence.SequenceType type) => BitConverter.GetBytes((ushort)(type switch {
            ACE.Server.Network.Sequence.SequenceType.ObjectPosition => 0x3344,
            ACE.Server.Network.Sequence.SequenceType.ObjectVisualDesc => 0x4568,
            ACE.Server.Network.Sequence.SequenceType.ObjectState => 0x3457,
            ACE.Server.Network.Sequence.SequenceType.ObjectTeleport => 0x5567,
            ACE.Server.Network.Sequence.SequenceType.ObjectMovement => 0x1234,
            ACE.Server.Network.Sequence.SequenceType.ObjectServerControl => 0x2346,
            ACE.Server.Network.Sequence.SequenceType.ObjectVector => 0x3456,
            ACE.Server.Network.Sequence.SequenceType.Motion => 0x4567,
            _ => 0,
        }));
        public byte[] GetNextSequence(ACE.Server.Network.Sequence.SequenceType type, object property) => new byte[] { 0x7f };
    }
    public partial class WorldObject {
        public ObjectGuid Guid = new(0x50000001);
        public string Name = "Synthetic";
        public Position Location = MovementHarness.SyntheticPosition();
        public Placement? Placement;
        public HarnessPhysics PhysicsObj;

        public HarnessSequences Sequences = new();
    }
}
namespace ACE.Server.WorldObjects.Entity {
    public sealed class CreatureAttribute {
        public PropertyAttribute Attribute = (PropertyAttribute)2;
        public uint Ranks = 3, StartingValue = 4, ExperienceSpent = 5;
    }
    public sealed class CreatureVital {
        public Vital Vital = (Vital)2;
        public uint Ranks = 3, StartingValue = 4, ExperienceSpent = 5, Current = 6;
    }
    public sealed class HarnessPropertiesSkill {
        public uint ResistanceAtLastCheck = 7;
        public double LastUsedTime = 12.25;
    }
    public sealed class CreatureSkill {
        public Skill Skill = (Skill)2;
        public ushort Ranks = 3;
        public SkillAdvancementClass AdvancementClass = (SkillAdvancementClass)4;
        public uint ExperienceSpent = 5, InitLevel = 6;
        public HarnessPropertiesSkill PropertiesSkill = new();
    }
}

namespace ACE.Server.Network.Managers {
    public static class InboundMessageManager {
        public static GameActionType LastAction;
        public static byte[] LastPayload;
        public static long Consumed;
        public static void HandleGameAction(GameActionType action, ClientMessage message, Session session) {
            LastAction = action; Consumed = message.Data.Position;
            LastPayload = message.Payload.ReadBytes((int)(message.Data.Length-message.Data.Position));
        }
    }
}
