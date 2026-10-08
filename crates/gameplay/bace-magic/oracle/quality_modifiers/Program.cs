// Deterministic oracle inputs only. AGPL-3.0-only.
using ACE.Entity.Enum;
using System.Globalization;
using System.Threading;
public class PropertiesEnchantmentRegistry {public int SpellId;public uint SpellCategory;public uint PowerLevel;public uint LayerId;public EnchantmentTypeFlags StatModType;public uint StatModKey;public double StartTime;}
class Program {static void Main(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;var r=new Random(431);var aura=(int)SpellId.BloodDrinkerSelf8;
 for(int scenario=0;scenario<512;scenario++) {var flags=(uint)((scenario%2==0?1:2)|(scenario%3==0?0x4000:0x8000));uint key=(uint)(scenario%6+1);var entries=new List<PropertiesEnchantmentRegistry>();var sets=new HashSet<int>();
 for(int i=0;i<32;i++){int spell=i==0?aura:i+1;var type=flags|(i%3==0?0x2000u:0x1000u);if(i%7==0)type|=0x800000;if(i%11==0)type^=3;bool set=i%5==0;if(set)sets.Add(spell);entries.Add(new(){SpellId=spell,SpellCategory=(uint)r.Next(1,8),PowerLevel=(uint)r.Next(1,4),LayerId=(uint)i,StatModType=(EnchantmentTypeFlags)type,StatModKey=i%3==0?0:(i%4==0?key+1:key),StartTime=-r.Next(0,3)});}
 var result=entries.GetEnchantmentsTopLayerByStatModType((EnchantmentTypeFlags)flags,key,new ReaderWriterLockSlim(),sets,true);
 var input=string.Join(";",entries.Select(e=>$"{e.SpellId},{e.SpellCategory},{e.PowerLevel},{(uint)e.StatModType},{e.StatModKey},{e.StartTime},{(sets.Contains(e.SpellId)?1:0)},{(e.SpellId==aura?1:0)}"));Console.WriteLine($"{flags}\t{key}\t{input}\t{string.Join(',',result.Select(e=>e.SpellId))}");
 }}}
