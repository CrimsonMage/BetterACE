using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.Json;
using ACE.DatLoader;
using ACE.DatLoader.FileTypes;

// Unused dependency of official Frame's convenience constructor, not its parser.
namespace ACE.Entity { public class Position { public System.Numerics.Vector3 Pos; public System.Numerics.Quaternion Rotation; } }
class Program
{
    static object Decode<T>(byte[] bytes) where T : FileType, new()
    {
        var item = new T(); using var reader = new BinaryReader(new MemoryStream(bytes));
        item.Unpack(reader);
        if (reader.BaseStream.Position != bytes.Length) throw new Exception("oracle did not consume full synthetic record");
        return new { bytes = Convert.ToHexString(bytes), decoded = item };
    }
    static byte[] Make(Action<BinaryWriter> write)
    {
        using var stream = new MemoryStream(); using var writer = new BinaryWriter(stream);
        write(writer); return stream.ToArray();
    }
    static void Align(BinaryWriter writer) { while (writer.BaseStream.Position % 4 != 0) writer.Write((byte)0xCC); }
    static void PString(BinaryWriter writer, string value)
    {
        var bytes = Encoding.UTF8.GetBytes(value); writer.Write((ushort)bytes.Length); writer.Write(bytes); Align(writer);
    }
    static void Count(BinaryWriter writer, uint count)
    {
        if (count < 128) writer.Write((byte)count);
        else if (count < 16384) { writer.Write((byte)(0x80 | (count >> 8))); writer.Write((byte)count); }
        else { writer.Write((byte)(0xC0 | (count >> 24))); writer.Write((byte)(count >> 16)); writer.Write((ushort)count); }
    }
    static void Words(BinaryWriter writer, params uint[] values) { Count(writer,(uint)values.Length); foreach (var v in values) writer.Write(v); }
    static void Appearance(BinaryWriter writer, bool populated)
    {
        Align(writer); writer.Write((byte)0x11);
        writer.Write((byte)(populated ? 1 : 0)); writer.Write((byte)(populated ? 1 : 0)); writer.Write((byte)(populated ? 1 : 0));
        if (populated) {
            writer.Write((ushort)7); // palette resource short ID
            writer.Write((ushort)0x8001); writer.Write((ushort)0x2345); // long subpalette ID
            writer.Write((byte)3); writer.Write((byte)0); // offset 24, colors 2048
            writer.Write((byte)4); writer.Write((ushort)10); writer.Write((ushort)11);
            writer.Write((byte)5); writer.Write((ushort)12);
        }
        Align(writer);
    }
    static byte[] Xp(bool populated) => Make(writer => {
        writer.Write(0x0E000018u);
        foreach (var count in new[] { populated ? 2 : 0, 0, populated ? 1 : 0, 0, populated ? 2 : 0 }) writer.Write(count);
        foreach (var value in populated ? new uint[] {0, 10, 30} : new uint[] {0}) writer.Write(value);
        writer.Write(0u);
        foreach (var value in populated ? new uint[] {0, 526} : new uint[] {0}) writer.Write(value);
        writer.Write(0u);
        foreach (var value in populated ? new ulong[] {0, 1234, 0x123456789ABCul} : new ulong[] {0}) writer.Write(value);
        foreach (var value in populated ? new uint[] {0, 1, 2} : new uint[] {0}) writer.Write(value);
    });
    static byte[] Skills() => Make(writer => {
        writer.Write(0x0E000004u); writer.Write((ushort)2); writer.Write((ushort)32);
        foreach (var id in new uint[] {6, 50}) {
            writer.Write(id); PString(writer, id == 6 ? "Synthetic café description" : ""); PString(writer, id == 6 ? "Défense" : "Synthetic");
            writer.Write(0x06000001u + id); writer.Write(id == 6 ? 6 : -1); writer.Write(12);
            writer.Write(1u); writer.Write(1u); writer.Write(2u);
            foreach (var value in new uint[] {2, 1, 3, 4, 1, 4}) writer.Write(value);
            writer.Write(0.95); writer.Write(0.25); writer.Write(1.5);
        }
    });
    static byte[] Character(bool populated) => Make(writer => {
        writer.Write(0x0E000002u); writer.Write(123u);
        Count(writer, populated ? 1u : 0u);
        if (populated) {
            writer.Write("Synthetic 🙂 area"); Count(writer,1); writer.Write(0x12340001u);
            foreach (var value in new float[] {1,2,3,1,0,0,0}) writer.Write(value);
        }
        writer.Write((byte)1); Count(writer, populated ? 1u : 0u);
        if (!populated) return;
        writer.Write(7u); writer.Write("Synthetic heritage");
        foreach (var value in new uint[] {0x06000001,0x02000001,0x02000002,330,52}) writer.Write(value);
        Words(writer,0); Words(writer);
        Count(writer,2);
        writer.Write(6u); writer.Write(6); writer.Write(12);
        writer.Write(50u); writer.Write(-1); writer.Write(-1);
        Count(writer,1); writer.Write("Synthetic template"); writer.Write(0x06000003u); writer.Write(7u);
        foreach (var value in new uint[] {10,20,30,40,50,60}) writer.Write(value);
        Words(writer,6); Words(writer,50);
        writer.Write((byte)1); Count(writer,1); writer.Write(1);
        writer.Write(new string('N',130));
        foreach (var value in new uint[] {100,0x02000001,0x20000001,0x06000001,0x04000001,0x0F000001,0x09000001,0x09000002,0x30000001}) writer.Write(value);
        Appearance(writer,true);
        Count(writer,130); for (uint i=0; i<130; i++) writer.Write(0x04000000u+i);
        Count(writer,1); writer.Write(0x06000002u); writer.Write((byte)1); writer.Write(0x02000003u); Appearance(writer,false);
        Words(writer,0x04000001);
        Count(writer,1); writer.Write(0x06000001u); writer.Write(0x06000002u); Appearance(writer,false); Appearance(writer,true);
        for (int i=0; i<2; i++) { Count(writer,1); writer.Write(0x06000003u); Appearance(writer,false); }
        for (int i=0; i<4; i++) { Count(writer,1); writer.Write("Synthetic gear"); writer.Write(0x10000001u); writer.Write(42u); }
        Words(writer,0x04000001);
    });
    static void Main()
    {
        var result = new { xp = new[] {Decode<XpTable>(Xp(false)), Decode<XpTable>(Xp(true))},
            skills = Decode<SkillTable>(Skills()), chargen = new[] {Decode<CharGen>(Character(false)), Decode<CharGen>(Character(true))} };
        Console.WriteLine(JsonSerializer.Serialize(result, new JsonSerializerOptions { IncludeFields = true }));
    }
}
