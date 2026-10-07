using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Numerics;
using ACE.Entity;
using ACE.Entity.Enum;
using ACE.Server.Entity;
using ACE.Server.Network;
using ACE.Server.Network.Enum;
using ACE.Server.Network.Structure;
using ACE.Server.Network.GameMessages;
using ACE.Server.Network.GameMessages.Messages;
using ACE.Server.Network.GameAction;
using ACE.Server.Network.GameAction.Actions;
using ACE.Server.WorldObjects;

static class MovementHarness {
    public static Position SyntheticPosition() {
        using var ms = new MemoryStream(); using var writer = new BinaryWriter(ms);
        writer.Write(0x12340001u); foreach (var value in new[] {1.25f,-2.5f,3.75f,0.1f,0.2f,0.3f,0.4f}) writer.Write(value);
        ms.Position = 0; return new Position(new BinaryReader(ms));
    }
    static object Message(GameMessage message) => new { bytes = Convert.ToHexString(message.Data.ToArray()), group = (uint)message.Group };
    static void Position(BinaryWriter writer) { writer.Write(0x12340001u); foreach (var value in new[] { 1.25f, -2.5f, 3.75f, 1f, 2f, 3f, 4f }) writer.Write(value); }
    static void Epochs(BinaryWriter writer) { foreach (ushort value in new ushort[] { 0x1122, 0x3344, 0x5566, 0x7788 }) writer.Write(value); }
    static object Pose(ACE.Entity.Position p) => new { cell = p.Cell, origin = new[] {p.PositionX,p.PositionY,p.PositionZ}, rotation = new[] {p.RotationW,p.RotationX,p.RotationY,p.RotationZ} };
    static object EpochValues(ushort a, ushort b, ushort c, ushort d) => new[] {a,b,c,d};
    public static object Run() {
        var wo = new WorldObject();
        var positions = Enumerable.Range(0,128).Select(mask => {
            var pack = new PositionPack { Flags = (PositionFlags)mask, Origin = new Origin(0x12340001, new Vector3(1.25f,-2.5f,3.75f)),
                Rotation = new Quaternion((mask & 16) != 0 ? 0 : 2, (mask & 32) != 0 ? 0 : 3, (mask & 64) != 0 ? 0 : 4, (mask & 8) != 0 ? 0 : 1),
                Velocity = new Vector3(5,6,7), PlacementID = (Placement)8,
                InstanceSequence = BitConverter.GetBytes((ushort)0x1122), PositionSequence = BitConverter.GetBytes((ushort)0x3344),
                TeleportSequence = BitConverter.GetBytes((ushort)0x5566), ForcePositionSequence = BitConverter.GetBytes((ushort)0x7788) };
            using var ms = new MemoryStream(); using var writer = new BinaryWriter(ms); writer.Write(pack);
            return new { flags = mask, bytes = Convert.ToHexString(ms.ToArray()) };
        }).ToArray();
        wo.PhysicsObj = new HarnessPhysics { Velocity = new Vector3(5,6,7), Omega = new Vector3(8,9,10), TransientState = ACE.Server.Physics.TransientStateFlags.OnWalkable };
        wo.Placement = (Placement)8;
        var outputs = new Dictionary<string, object> {
            ["position"] = Message(new GameMessageUpdatePosition(wo)),
            ["admin_position"] = Message(new GameMessageUpdatePosition(wo, true)),
            ["vector"] = Message(new GameMessageVectorUpdate(wo)),
            ["autonomous"] = Message(new GameMessageAutonomousPosition(new Player())),
        };
        var session = new Session { Player = new HarnessPlayer() };
        var jumps = new[] { false, true }.Select(trailing => {
            using var ms = new MemoryStream(); using var writer = new BinaryWriter(ms);
            writer.Write((uint)GameMessageOpcode.GameAction); writer.Write(42u); writer.Write((uint)GameActionType.Jump);
            writer.Write(.75f); foreach (var value in new[] {1.25f,-2.5f,3.75f}) writer.Write(value); Epochs(writer);
            writer.Write(0x50000002u); writer.Write(123u); if (trailing) writer.Write(new byte[] {0xa5,0x5a});
            var bytes = ms.ToArray(); var message = new ClientMessage(bytes); message.Payload.ReadUInt32(); message.Payload.ReadUInt32();
            GameActionJump.Handle(message,session); var jump = session.Player.LastJump;
            return new { bytes = Convert.ToHexString(bytes), extent = jump.Extent, velocity = new[] {jump.Velocity.X,jump.Velocity.Y,jump.Velocity.Z}, epochs = EpochValues(jump.InstanceSequence,jump.ServerControlSequence,jump.TeleportSequence,jump.ForcePositionSequence), consumed = message.Data.Position, trailing_bytes = message.Data.Length-message.Data.Position };
        }).ToArray();
        var autonomous = new[] {0,1,2}.Select(contact => {
            using var ms = new MemoryStream(); using var writer = new BinaryWriter(ms);
            writer.Write((uint)GameMessageOpcode.GameAction); writer.Write(42u); writer.Write((uint)GameActionType.AutonomousPosition);
            Position(writer); Epochs(writer); writer.Write((byte)contact); writer.Write(new byte[3]);
            writer.Write(new byte[] {0xa5,0x5a});
            var bytes = ms.ToArray(); var message = new ClientMessage(bytes); message.Payload.ReadUInt32(); message.Payload.ReadUInt32();
            GameActionAutonomousPosition.Handle(message,session);
            return new { bytes = Convert.ToHexString(bytes), pose = Pose(session.Player.RequestedLocation), contact = session.Player.LastContact, consumed = message.Data.Position, trailing_bytes = message.Data.Length-message.Data.Position };
        }).ToArray();
        var rawMasks = new uint[] {0,1,2,4,8,16,32,64,128,256,512,1024,2047};
        var raw = rawMasks.SelectMany(mask => new[] {0,2}.Select(count => {
            using var ms = new MemoryStream(); using var writer = new BinaryWriter(ms);
            writer.Write((uint)GameMessageOpcode.GameAction); writer.Write(42u); writer.Write((uint)GameActionType.MoveToState);
            writer.Write(mask | ((uint)count << 11));
            for (var bit=0;bit<11;bit++) if ((mask & (1u<<bit)) != 0) {
                if (bit == 4 || bit == 7 || bit == 10) writer.Write((bit+1)*.25f); else writer.Write((uint)(bit+1));
            }
            for (var i=0;i<count;i++) { writer.Write(unchecked((ushort)MotionCommand.Wave)); writer.Write((ushort)(i == 0 ? 0x1234 : 0xabcd)); writer.Write(1.0f); }
            Position(writer); Epochs(writer); writer.Write((byte)3); writer.Write(new byte[3]); writer.Write(new byte[] {0xa5,0x5a});
            var bytes = ms.ToArray(); var message = new ClientMessage(bytes); message.Payload.ReadUInt32(); message.Payload.ReadUInt32();
            var state = new MoveToState(wo,message.Payload); var motion = state.RawMotionState;
            return new { bytes = Convert.ToHexString(bytes), flags = (uint)motion.Flags,
                words = new uint[] {(uint)motion.CurrentHoldKey,(uint)motion.CurrentStyle,(uint)motion.ForwardCommand,(uint)motion.ForwardHoldKey,(uint)motion.SidestepCommand,(uint)motion.SidestepHoldKey,(uint)motion.TurnCommand,(uint)motion.TurnHoldKey},
                speeds = new[] {motion.ForwardSpeed,motion.SidestepSpeed,motion.TurnSpeed},
                commands = motion.Commands?.Select(command => new { raw_command = (ushort)command.MotionCommand, sequence = command.ServerActionSequence, autonomous = command.IsAutonomous, speed = command.Speed }).ToArray(),
                pose = Pose(state.Position), epochs = EpochValues(state.InstanceSequence,state.ServerControlSequence,state.TeleportSequence,state.ForcePositionSequence),
                contact = state.Contact, standing_long_jump = state.StandingLongJump, consumed = message.Data.Position, trailing_bytes = message.Data.Length-message.Data.Position };
        })).ToArray();
        var motions = new List<object>();
        foreach (var mask in Enumerable.Range(0,128)) {
            foreach (var autonomousMotion in new[] {false,true}) {
                var motion = new MovementData(wo) { MovementType = MovementType.Invalid, MotionFlags = MotionFlags.StickToObject | MotionFlags.StandingLongJump, CurrentStyle = (MotionStance)61, IsAutonomous = autonomousMotion };
                var state = new InterpretedMotionState { Flags = (MovementStateFlag)mask, CurrentStyle = (MotionStance)62, ForwardCommand = MotionCommand.RunForward, SidestepCommand = MotionCommand.SideStepRight, TurnCommand = MotionCommand.TurnRight, ForwardSpeed = 1.25f, SidestepSpeed = -2.5f, TurnSpeed = 3.75f,
                    Commands = new List<MotionItem> { new(wo, MotionCommand.Wave, 1.5f) { IsAutonomous = autonomousMotion } } };
                motion.Invalid = new MovementInvalid(motion,state) { StickyObject = new ObjectGuid(0x80000001) };
                motions.Add(new { state_flags = mask, autonomous = autonomousMotion, style = (ushort)motion.CurrentStyle,
                    command = unchecked((ushort)MotionCommand.Wave), forward = (ushort)state.ForwardCommand, sidestep = (ushort)state.SidestepCommand, turn = (ushort)state.TurnCommand,
                    message = Message(new GameMessageUpdateMotion(wo,motion)) });
            }
        }
        var parameters = new MoveToParameters { MovementParameters = (MovementParams)0x1efff, DistanceToObject = .6f, MinDistance = .1f, FailDistance = 100f, Speed = 1.5f, WalkRunThreshold = 15f, DesiredHeading = 90f };
        var directed = new[] {MovementType.MoveToObject,MovementType.MoveToPosition,MovementType.TurnToObject,MovementType.TurnToHeading}.Select(type => {
            var motion = new Motion { MovementType = type, Stance = (MotionStance)61, Position = wo.Location, TargetGuid = new ObjectGuid(0x80000001), MoveToParameters = parameters, RunRate = 2.5f, DesiredHeading = 180f };
            return new { type = (byte)type, message = Message(new GameMessageUpdateMotion(wo,motion)) };
        }).ToArray();
        var empty = new MovementData(wo) { IsAutonomous = true, MovementType = MovementType.Invalid };
        empty.Invalid = new MovementInvalid(empty) { State = new InterpretedMotionState { Flags = 0 } };
        outputs["empty_motion"] = Message(new GameMessageUpdateMotion(wo,empty));
        return new { positions, outputs, jumps, autonomous, raw, motions, directed };
    }
}
// Domain-only inputs. Wire readers/writers and enum definitions are compiled
// unchanged from pinned source. Vector3.Write is extracted verbatim and hashed.
namespace ACE.Server.Physics { public enum TransientStateFlags { OnWalkable = 2 } }
namespace ACE.Server.WorldObjects {
    public sealed class HarnessPhysics { public Vector3 Velocity, Omega; public ACE.Server.Physics.TransientStateFlags TransientState; }
    public class Creature : WorldObject { public float GetRunRate() => 1.5f; }
    public partial class Player : Creature { }
}
namespace ACE.Server.Entity {
    public class Motion {
        public bool IsAutonomous;
        public MovementType MovementType;
        public MotionFlags MotionFlags;
        public MotionStance Stance;
        public ObjectGuid TargetGuid;
        public Position Position;
        public MoveToParameters MoveToParameters = new();
        public float RunRate, DesiredHeading;
        public InterpretedMotionState MotionState = new();
    }
}
namespace log4net {
    public interface ILog { void Error(string value); }
    public sealed class HarnessLog : ILog { public void Error(string value) { } }
    public static class LogManager { public static ILog GetLogger(Type type) => new HarnessLog(); }
}
