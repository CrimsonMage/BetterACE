using System;
using System.IO;
using System.Linq;
using System.Text.Json;
using ACE.DatLoader.FileTypes;
// Inert dependencies of unused convenience constructors / diagnostic logging.
namespace ACE.Entity { public class Position { public System.Numerics.Vector3 Pos; public System.Numerics.Quaternion Rotation; } }
namespace log4net { public interface ILog { void Warn(object text); } public class Quiet:ILog {public void Warn(object text){}} public static class LogManager {public static ILog GetLogger(Type t)=>new Quiet();} }
namespace ACE.Common { public class UnusedNamespace {} }
class Program {
    static byte[] Make(Action<BinaryWriter> write) {using var stream=new MemoryStream();using var w=new BinaryWriter(stream);write(w);return stream.ToArray();}
    static object Decode<T>(byte[] bytes) where T:FileType,new() {var value=new T();using var r=new BinaryReader(new MemoryStream(bytes));value.Unpack(r);if(r.BaseStream.Position!=bytes.Length)throw new Exception("Trailing oracle input");return new {bytes=Convert.ToHexString(bytes),decoded=value};}
    static void Floats(BinaryWriter w,params float[] values){foreach(var v in values)w.Write(v);}
    static void Main() {
        var vitals=Make(w=>{w.Write(0x0E000003u);foreach(var divisor in new uint[]{2,1,1}){w.Write(0u);w.Write(1u);w.Write(0u);w.Write(divisor);w.Write(2u);w.Write(0u);}});
        var palette=Make(w=>{w.Write(0x04000001u);w.Write(3u);w.Write(0xFFFF0000u);w.Write(0xFF00FF00u);w.Write(0x000000FFu);});
        var set=Make(w=>{w.Write(0x0F000001u);w.Write(2u);w.Write(0x04000001u);w.Write(0x04000002u);});
        var clothing=Make(w=>{w.Write(0x10000001u);w.Write((ushort)1);w.Write((ushort)16);w.Write(0x02000001u);w.Write(1u);w.Write(0u);w.Write(0x01000001u);w.Write(1u);w.Write(0x05000001u);w.Write(0x05000002u);w.Write((ushort)1);w.Write((ushort)16);w.Write(2u);w.Write(0x06000001u);w.Write(1u);w.Write(1u);w.Write(8u);w.Write(16u);w.Write(0x0F000001u);});
        var surface=Make(w=>{w.Write(2u);w.Write(0x05000001u);w.Write(0x04000001u);Floats(w,0,0.25f,0.75f);});
        var textures=Make(w=>{w.Write(0x05000001u);w.Write(0u);w.Write((byte)1);w.Write(1u);w.Write(0x06000001u);});
        var setup=Make(w=>{w.Write(0x02000001u);w.Write(2u);w.Write(1u);w.Write(0x01000001u);Floats(w,2,3,4);w.Write(0u);w.Write(0u);w.Write(1u);w.Write(0u);Floats(w,1,2,3,1,0,0,0);w.Write(0u);w.Write(0u);w.Write(0u);for(int i=0;i<12;i++)w.Write(0f);w.Write(0u);w.Write(0x03000001u);w.Write(0u);w.Write(0x09000001u);w.Write(0u);w.Write(0u);});
        var gfx=Make(w=>{w.Write(0x01000001u);w.Write(2u);w.Write((byte)1);w.Write(0x08000001u);w.Write(1u);w.Write(3u);for(ushort i=0;i<3;i++){w.Write(i);w.Write((ushort)1);Floats(w,i==1?1:0,i==2?1:0,0,0,0,1,i==1?1:0,i==2?1:0);}Floats(w,0,0,0);w.Write((byte)1);w.Write((ushort)4);w.Write((byte)3);w.Write((byte)0);w.Write(1u);w.Write((short)0);w.Write((short)-1);w.Write((ushort)0);w.Write((ushort)1);w.Write((ushort)2);w.Write(new byte[]{0,0,0});w.Write(0x4C454146u);w.Write(0);});
        Console.WriteLine(JsonSerializer.Serialize(new{vitals=Decode<SecondaryAttributeTable>(vitals),palette=Decode<Palette>(palette),palette_set=Decode<PaletteSet>(set),clothing=Decode<ClothingTable>(clothing),surface=Decode<Surface>(surface),textures=Decode<SurfaceTexture>(textures),setup=Decode<SetupModel>(setup),gfx=Decode<GfxObj>(gfx)},new JsonSerializerOptions{IncludeFields=true}));
    }
}
