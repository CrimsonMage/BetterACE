using System;
using System.Linq;
using System.IO;
using System.Text;
using ACE.Server.Network.Packets;
using System.Text.Json;
using ACE.Common.Cryptography;
using ACE.Server.Network;
class Program
{
    static void Main()
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        var header = new PacketHeader { Sequence = 0x12345678, Flags = PacketHeaderFlags.BlobFragments | PacketHeaderFlags.EncryptedChecksum, Checksum = 0xdeadbeef, Id = 0x1122, Time = 0x3344, Size = 0x01d0, Iteration = 1 };
        var hb = new byte[20]; header.Pack(hb);
        var fragment = new PacketFragmentHeader { Sequence = 42, Id = 0x80000000, Count = 3, Size = 464, Index = 1, Queue = 10 };
        var fb = new byte[16]; fragment.Pack(fb);
        var hashes = Enumerable.Range(0, 33).Select(length => { var bytes = Enumerable.Range(0, length).Select(i => (byte)(i * 37 + 11)).ToArray(); return new { input = Convert.ToHexString(bytes), hash = Hash32.Calculate(bytes, bytes.Length) }; }).ToArray();
        var keys = new uint[] { 0, 1, 0x12345678, 0xffffffff }.Select(seed => { var rng = new ISAAC(BitConverter.GetBytes(seed)); return new { seed, keys = Enumerable.Range(0, 600).Select(_ => rng.Next()).ToArray() }; }).ToArray();
        var strings = new[] { "", "a", "ab", "abc", "Euro € café" }.Select(value => { using var ms = new MemoryStream(); using var writer = new BinaryWriter(ms); writer.WriteString16L(value); return new { value, bytes = Convert.ToHexString(ms.ToArray()) }; }).ToArray();
        var packed = new uint[] { 0, 1, 32767, 32768, 0x12345678, 0x7fffffff }.Select(value => { using var ms = new MemoryStream(); using var writer = new BinaryWriter(ms); writer.WritePackedDword(value); return new { value, bytes = Convert.ToHexString(ms.ToArray()) }; }).ToArray();
        var connect = new PacketOutboundConnectRequest(123.25, 0x123456789abcdef0, 42, BitConverter.GetBytes(0x87654321u), BitConverter.GetBytes(0x11223344u));
        var connectPayload = Convert.ToHexString(connect.Data.ToArray());
        var packets = new uint[] { 0, 0x12345678 }.Select(key => {
            var packet = new ServerPacket(); packet.Header.Sequence = 2; packet.Header.Flags = PacketHeaderFlags.BlobFragments | PacketHeaderFlags.AckSequence;
            if (key != 0) packet.Header.Flags |= PacketHeaderFlags.EncryptedChecksum;
            packet.Header.Id = 7; packet.Header.Time = 19; packet.Header.Iteration = 1;
            packet.InitializeDataWriter(); packet.DataWriter.Write(12u); packet.IssacXor = key;
            var part = new ServerPacketFragment(new byte[] { 1, 2, 3, 4, 5 }); part.Header.Sequence = 1; part.Header.Id = 0x80000000; part.Header.Count = 1; part.Header.Index = 0; part.Header.Queue = 9; packet.Fragments.Add(part);
            var buffer = new byte[484]; packet.CreateReadyToSendPacket(buffer, out var size);
            var first = Convert.ToHexString(buffer.AsSpan(0, size)); packet.Header.Flags |= PacketHeaderFlags.Retransmission; packet.CreateReadyToSendPacket(buffer, out size);
            return new { key, bytes = first, retransmit = Convert.ToHexString(buffer.AsSpan(0, size)) };
        }).ToArray();
        var optionMasks = new uint[] { 0x100, 0x1000, 0x2000, 0x4000, 0x400000, 0x1000000, 0x2000000, 0x8000000, 0xb407100 };
        var optional = optionMasks.Select(mask => {
            using var ms = new MemoryStream(); using var writer = new BinaryWriter(ms);
            if ((mask & 0x100) != 0) writer.Write(new byte[] { 1,2,3,4,5,6,7,8 });
            if ((mask & 0x1000) != 0) { writer.Write(2u); writer.Write(7u); writer.Write(8u); }
            if ((mask & 0x2000) != 0) { writer.Write(1u); writer.Write(9u); }
            if ((mask & 0x4000) != 0) writer.Write(10u);
            if ((mask & 0x400000) != 0) writer.Write(new byte[] { 8,7,6,5,4,3,2,1 });
            if ((mask & 0x1000000) != 0) writer.Write(12.25);
            if ((mask & 0x2000000) != 0) writer.Write(13.5f);
            if ((mask & 0x8000000) != 0) { writer.Write(14u); writer.Write((ushort)15); }
            ms.Position = 0; var parsed = new PacketHeaderOptional(); parsed.Unpack(new BinaryReader(ms), new PacketHeader { Flags = (PacketHeaderFlags)mask });
            return new { flags = mask, bytes = Convert.ToHexString(ms.ToArray()), size = parsed.Size, hash = parsed.CalculateHash32(), valid = parsed.IsValid, ack = parsed.AckSequence, time = parsed.TimeSynch, echo = parsed.EchoRequestClientTime, flow_bytes = parsed.FlowBytes, flow_interval = parsed.FlowInterval };
        }).ToArray();
        var loginInputs = new[] {
            ("Account", "synthetic-password", 2u, true),
            ("Élodie", "synthétique", 2u, true),
            ("astral🙂", "pw🙂", 2u, true),
            ("boundary255", new string('x',255), 2u, true),
            ("boundary256", new string('x',256), 2u, true),
            ("unpadded", "fixture", 2u, false),
            ("ticket", "synthetic-ticket", 0x40000002u, true),
            ("status-only", "", 1u, true),
            ("empty-password", "", 2u, true)
        };
        var logins = loginInputs.Select(input => {
            var payload = BuildLogin(input.Item1,input.Item2,input.Item3,input.Item4);
            var decoded = new PacketInboundLoginRequest(new ClientPacket(payload));
            return new { bytes = Convert.ToHexString(payload), version = decoded.ClientVersion, account = decoded.Account, type = (uint)decoded.NetAuthType, timestamp = decoded.Timestamp, password = decoded.Password, ticket = decoded.GlsTicket };
        }).ToArray();
        var byteCounter = new ACE.Server.Network.Sequence.ByteSequence(false);
        var objectCounter = new ACE.Server.Network.Sequence.UShortSequence();
        var motionCounter = new ACE.Server.Network.Sequence.UShortSequence(1, 0x7fff);
        var sequences = new {
            property = Enumerable.Range(0, 258).Select(_ => (uint)byteCounter.NextValue).ToArray(),
            object_boundary = Enumerable.Range(0, 65537).Select(_ => (uint)objectCounter.NextValue).ToArray().Skip(65533).ToArray(),
            motion_boundary = Enumerable.Range(0, 32769).Select(_ => (uint)motionCounter.NextValue).ToArray().Skip(32765).ToArray()
        };
        Console.WriteLine(JsonSerializer.Serialize(new { header = Convert.ToHexString(hb), header_hash = header.CalculateHash32(), fragment = Convert.ToHexString(fb), hashes, isaac = keys, strings, packed, connect = connectPayload, packets, optional, logins, sequences }));
    }
    static void LoginString16(BinaryWriter writer, string value)
    {
        writer.Write((ushort)value.Length); writer.Write(Encoding.UTF8.GetBytes(value));
        writer.Write(new byte[(4-(value.Length+2)%4)%4]);
    }
    static byte[] BuildLogin(string account, string secret, uint type, bool padding)
    {
        using var ms = new MemoryStream(); using var writer = new BinaryWriter(ms);
        LoginString16(writer,"1802"); writer.Write(999u); writer.Write(type); writer.Write(1u); writer.Write(12345u);
        LoginString16(writer,account); LoginString16(writer,"");
        if (type == 2 || type == 0x40000002) {
            if (secret.Length == 0) writer.Write(0u);
            else {
                var prefix = secret.Length > 255 ? 2 : 1;
                writer.Write((uint)(secret.Length + prefix)); writer.Write((byte)secret.Length);
                if (prefix == 2) writer.Write((byte)(secret.Length >> 8));
                writer.Write(Encoding.UTF8.GetBytes(secret));
                if (padding) writer.Write(new byte[(4-secret.Length%4)%4]);
            }
        }
        return ms.ToArray();
    }

}

// Harness-only input adapter: the official LoginRequest decoder only consumes
// DataReader. This tests its payload decoder, not ClientPacket.Unpack or routing.
namespace ACE.Server.Network
{
    public sealed class ClientPacket
    {
        public BinaryReader DataReader { get; }
        public ClientPacket(byte[] payload) { DataReader = new BinaryReader(new MemoryStream(payload)); }
    }
}
