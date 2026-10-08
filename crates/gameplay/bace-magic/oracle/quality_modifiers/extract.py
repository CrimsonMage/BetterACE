from pathlib import Path
import hashlib,re
out=Path(__file__).resolve().parent
root=out.parents[4]
ace=root/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
p=ace/'ACE.Entity/Models/PropertiesEnchantmentRegistryExtensions.cs'
s=p.read_text(encoding='utf-8-sig');signature='public static List<PropertiesEnchantmentRegistry> GetEnchantmentsTopLayerByStatModType(this ICollection<PropertiesEnchantmentRegistry> value, EnchantmentTypeFlags statModType, uint statModKey,'
a=s.index(signature);b=s.index('{',a);depth=1;i=b+1
while depth:
 depth+=(s[i]=='{')-(s[i]=='}');i+=1
start=s.index('private static HashSet<int> Level8AuraSelfSpells');end=s.index('};',start)+2
field=s[start:end]
spell=(ace/'ACE.Entity/Enum/SpellId.cs').read_text(encoding='utf-8-sig')
names=re.findall(r'SpellId\.(\w+)',field)
values={};ordinal=0
for line in spell.splitlines():
 m=re.match(r'\s*(\w+)\s*(?:=\s*(\d+))?\s*,',line)
 if not m:continue
 if m.group(2):ordinal=int(m.group(2))
 if m.group(1) in names:values[m.group(1)]=str(ordinal)
 ordinal+=1
assert len(values)==len(names)
(out/'Original.cs').write_text('// Unchanged official ACE method and field, AGPL-3.0-only, ACEmulator contributors.\nusing System;using System.Linq;using System.Collections.Generic;using System.Threading;using ACE.Entity.Enum;\npublic static class Original{'+field+'\n'+s[a:i]+'}\nnamespace ACE.Entity.Enum { public enum SpellId {'+','.join(f'{n}={v}' for n,v in values.items())+'}}')
flags=ace/'ACE.Entity/Enum/EnchantmentTypeFlags.cs';(out/'Flags.cs').write_bytes(flags.read_bytes())
(out/'source.sha256').write_text(''.join(f'{hashlib.sha256(x.read_bytes()).hexdigest()}  {x.relative_to(ace)}\n' for x in [p,flags,ace/'ACE.Entity/Enum/SpellId.cs']))
(out/'aura.txt').write_text(next(iter(values.values())))
