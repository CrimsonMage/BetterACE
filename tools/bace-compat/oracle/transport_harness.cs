using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using ACE.Common.Cryptography;
using ACE.Server.Network;
using ACE.Server.Network.GameMessages;
using ACE.Server.Network.Packets;

// Harness-only synthetic dependencies. No production accounts, assets or
// captured player packets are used. Expected algorithms are injected verbatim.
namespace log4net
{
    public class Logger { public bool IsDebugEnabled => false; public void DebugFormat(string value, params object[] args) {} }
    public interface ILog { bool IsDebugEnabled { get; } void DebugFormat(string value, params object[] args); }
    public class NullLogger : ILog { public bool IsDebugEnabled => false; public void DebugFormat(string value, params object[] args) {} }
    public static class LogManager
    {
        public static ILog GetLogger(params object[] args) => new NullLogger();
    }
}
namespace ACE.Server.Network.GameMessages
{
    public class GameMessage
    {
        public MemoryStream Data { get; }
        public GameMessageGroup Group { get; }
        public GameMessage(byte[] data, GameMessageGroup group) { Data = new MemoryStream(data); Group = group; }
    }
}
namespace ACE.Server.Network
{
    public static class Timers { public static double PortalYearTicks; }
    public static class NetworkStatistics
    {
        public static void S2C_Packets_Aggregate_Increment() {}
        public static void S2C_RequestsForRetransmit_Aggregate_Increment() {}
    }
    public static class CryptoSystem { public const uint MaximumEffortLevel = 256; }
    public enum SessionTerminationReason { AbnormalSequenceReceived }
    public class FakeSession
    {
        public string LoggingIdentifier => "fixture";
        public FakeNetwork Network => null;
        public object EndPointC2S => null;
        public string Account => "synthetic";
        public FakePlayer Player => null;
        public bool Terminated;
        public void Terminate(SessionTerminationReason reason) { Terminated = true; }
    }
    public class FakeNetwork { public ushort ClientId => 1; }
    public class FakePlayer { public string Name => "synthetic"; }
    public class FakeConnection
    {
        public Sequence.UIntSequence PacketSequence = new Sequence.UIntSequence(1);
        public uint FragmentSequence;
        public ISAAC IssacServer = new ISAAC(BitConverter.GetBytes(34u));
    }
    public class FakePacket { }
    public class NetworkSession
    {
        readonly log4net.ILog packetLog = new log4net.NullLogger();
        readonly FakeSession session = new FakeSession();
        readonly FakeConnection ConnectionData = new FakeConnection();
        readonly ConcurrentQueue<ServerPacket> packetQueue = new ConcurrentQueue<ServerPacket>();
        readonly ConcurrentDictionary<uint, ServerPacket> cachedPackets = new ConcurrentDictionary<uint, ServerPacket>();
        readonly ConcurrentDictionary<uint, FakePacket> outOfOrderPackets = new ConcurrentDictionary<uint, FakePacket>();
        readonly List<string> captured = new List<string>();
        readonly ushort ServerId = 1;
        uint lastReceivedPacketSequence = 1;
        const uint MaxNumNakSeqIds = 115;
        const int cachedPacketRetentionTime = 120;
        DateTime LastRequestForRetransmitTime;
        DateTime lastCachedPacketPruneTime;
        void EnqueueSend(ServerPacket packet) { packetQueue.Enqueue(packet); }
        void SendPacketRaw(ServerPacket packet)
        {
            // Large buffer observes upstream oversized-tail behavior rather than
            // silently fixing its calculations in the oracle.
            var buffer = new byte[4096];
            packet.CreateReadyToSendPacket(buffer, out var size);
            captured.Add(Convert.ToHexString(buffer.AsSpan(0, size)));
        }
        public string[] Bundle(int queue, int[] lengths, bool optional)
        {
            Timers.PortalYearTicks = 123.25;
            var bundle = new NetworkBundle { EncryptedChecksum = true };
            if (optional) { bundle.SendAck = true; bundle.TimeSync = true; bundle.ClientTime = 100.5f; }
            foreach (var length in lengths)
                bundle.Enqueue(new GameMessage(Enumerable.Range(0, length).Select(i => (byte)i).ToArray(), (GameMessageGroup)queue));
            SendBundle(bundle, (GameMessageGroup)queue);
            FlushPackets();
            return captured.ToArray();
        }
        public object InitialCounters()
        {
            Timers.PortalYearTicks = 123.25;
            ConnectionData.PacketSequence = new Sequence.UIntSequence(false);
            EnqueueSend(new PacketOutboundConnectRequest(123.25, 42, 1, BitConverter.GetBytes(34u), BitConverter.GetBytes(12u)));
            FlushPackets();
            var ack = new ServerPacket(); ack.Header.Flags = PacketHeaderFlags.AckSequence;
            ack.InitializeDataWriter(); ack.DataWriter.Write(1u); EnqueueSend(ack); FlushPackets();
            var sync = new NetworkBundle { EncryptedChecksum = true, TimeSync = true };
            SendBundle(sync, GameMessageGroup.InvalidQueue); FlushPackets();
            return captured.ToArray();
        }
        public object Reliability()
        {
            Bundle(9, new[] { 4, 448 }, false);
            var originals = captured.ToArray(); captured.Clear();
            var found = Retransmit(2); var missing = Retransmit(99);
            var replay = captured.ToArray(); captured.Clear();
            AcknowledgeSequence(3);
            var afterAck = cachedPackets.Keys.OrderBy(x => x).ToArray();
            lastReceivedPacketSequence = 1;
            outOfOrderPackets.TryAdd(4, new FakePacket());
            DoRequestForRetransmission(7); FlushPackets();
            var nak = captured.ToArray(); captured.Clear();
            Timers.PortalYearTicks = 243; PruneCachedPackets();
            var retainedAt120 = cachedPackets.Keys.OrderBy(x => x).ToArray();
            Timers.PortalYearTicks = 244; PruneCachedPackets();
            var retainedAt121 = cachedPackets.Keys.OrderBy(x => x).ToArray();
            return new { originals, found, missing, replay, afterAck, nak, retainedAt120, retainedAt121 };
        }
        /* PINNED_METHODS */
    }
}
class Program
{
    static void Main()
    {
        var bundles = new List<object>();
        foreach (var queue in new[] { 3, 9 })
        foreach (var lengths in new[] { new[] { 100, 900, 4 }, new[] { 400, 100, 4 }, new[] { 4, 448 }, new[] { 4, 449 }, new[] { 4, 896 }, new[] { 448 }, new[] { 449 }, new[] { 896 }, new[] { 900 } })
            bundles.Add(new { queue, lengths, optional = false, packets = new NetworkSession().Bundle(queue, lengths, false) });
        foreach (var lengths in new[] { new[] { 100, 900, 4 }, new[] { 440 }, Array.Empty<int>() })
            bundles.Add(new { queue = 3, lengths, optional = true, packets = new NetworkSession().Bundle(3, lengths, true) });
        Console.WriteLine(JsonSerializer.Serialize(new { bundles, initialCounters = new NetworkSession().InitialCounters(), reliability = new NetworkSession().Reliability() }));
    }
}
