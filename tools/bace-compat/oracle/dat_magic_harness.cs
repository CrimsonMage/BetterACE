using System;using System.IO;using System.Text;using System.Text.Json;using System.Linq;using System.Collections.Generic;using ACE.DatLoader.FileTypes;using ACE.DatLoader.Entity;
class Program {
 static void Align(BinaryWriter w){while(w.BaseStream.Position%4!=0)w.Write((byte)0);}
 static void Text(BinaryWriter w,string text){byte[] b=Encoding.GetEncoding(1252).GetBytes(text);w.Write((ushort)b.Length);foreach(byte v in b)w.Write((byte)((v>>4)|(v<<4)));Align(w);}
 static byte[] BuildTable(){using var s=new MemoryStream();using var w=new BinaryWriter(s);w.Write(0x0e00000eu);w.Write((ushort)4);w.Write((ushort)32);
  for(uint i=0;i<4;i++){w.Write(i+100);string name=i==0?"Café – Flame":"Spell "+i;string desc="Synthetic spell";Text(w,name);Text(w,desc);w.Write(i+1);w.Write(0x06000001u);w.Write(37u);w.Write(0x4000u);w.Write(10u);w.Write(30f);w.Write(0.1f);w.Write(100u);w.Write(0.5f);w.Write(i);w.Write(0.25f);uint meta=new uint[]{3,1,12,7}[i];w.Write(meta);w.Write(i+100);if(meta==1||meta==12){w.Write(120.5);w.Write(0.25f);w.Write(-666f);}if(meta==7)w.Write(60.0);
   uint key=unchecked(SpellTable.ComputeHash(name)%0x12107680+SpellTable.ComputeHash(desc)%0xbeadcf45);foreach(uint comp in new uint[]{1,63,10,64,20,30,65,40})w.Write(unchecked(comp+key));w.Write(1u);w.Write(2u);w.Write(0u);w.Write(0.0);w.Write(0f);w.Write(i);w.Write(16u);w.Write(2u);
  }
  w.Write((ushort)1);w.Write((ushort)16);w.Write(7u);w.Write((ushort)2);w.Write((ushort)8);w.Write(1u);w.Write(2u);w.Write(100u);w.Write(101u);w.Write(3u);w.Write(1u);w.Write(102u);return s.ToArray();
 }
 static byte[] BuildComponents(){using var s=new MemoryStream();using var w=new BinaryWriter(s);w.Write(0x0e00000fu);w.Write((ushort)2);Align(w);for(uint i=0;i<2;i++){w.Write(i+1);Text(w,i==0?"Scarab":"Talisman");w.Write(i+1);w.Write(0x06000010u+i);w.Write(i+1);w.Write(i==0?0x80000000u:0x13000132u);w.Write(1.5f);Text(w,i==0?"Incantare":"Flame");w.Write(0.5f);}return s.ToArray();}
 static void Main(){Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);var tableBytes=BuildTable();var componentsBytes=BuildComponents();var table=new SpellTable();table.Unpack(new BinaryReader(new MemoryStream(tableBytes)));var components=new SpellComponentsTable();components.Unpack(new BinaryReader(new MemoryStream(componentsBytes)));
 var formulas=new List<object>();foreach(var account in new[]{"Alpha","Beta","Café"})foreach(uint spell in table.Spells.Keys)formulas.Add(new{account,spell,formula=SpellTable.GetSpellFormula(table,spell,account)});
 Console.WriteLine(JsonSerializer.Serialize(new{table=new{bytes=Convert.ToHexString(tableBytes).ToLowerInvariant(),spells=table.Spells,sets=table.SpellSet.ToDictionary(k=>k.Key,k=>k.Value.SpellSetTiers.ToDictionary(t=>t.Key,t=>t.Value.Spells))},components=new{bytes=Convert.ToHexString(componentsBytes).ToLowerInvariant(),entries=components.SpellComponents},formulas,hashes=new[]{"Alpha","Café","– €", ""}.Select(text=>new{text,value=SpellTable.ComputeHash(text)})}));}
}
