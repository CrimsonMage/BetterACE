#!/usr/bin/env python3
"""Independent pinned ACE magic serializers and scalar formula oracle."""
import argparse,hashlib,json,subprocess,tempfile,urllib.request
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
NET='Source/ACE.Server/Network/'
EVENTS=['MagicUpdateSpell','MagicUpdateEnchantment','MagicRemoveEnchantment','MagicUpdateMultipleEnchantments','MagicRemoveMultipleEnchantments','MagicPurgeEnchantments','MagicDispelEnchantment','MagicDispelMultipleEnchantments','MagicPurgeBadEnchantments']
FILES=[NET+'GameEvent/GameEventType.cs',NET+'GameMessageGroup.cs',NET+'Structure/Enchantment.cs',NET+'Structure/EnchantmentRegistry.cs',NET+'Structure/LayeredSpell.cs','Source/ACE.Server/WorldObjects/SkillCheck.cs','Source/ACE.Server/WorldObjects/Creature_Magic.cs','Source/ACE.Server/Entity/Spell.cs','Source/ACE.Entity/Enum/SpellId.cs','Source/ACE.Entity/Enum/EnchantmentCategory.cs']+[NET+'GameEvent/Events/GameEvent'+name+'.cs' for name in EVENTS]
def method(source,signature):
 start=source.index(signature);opening=source.index('{',start);depth=0
 for i in range(opening,len(source)):
  depth+=(source[i]=='{')-(source[i]=='}')
  if depth==0:return source[start:i+1]
 raise ValueError(signature)
def main():
 p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',default='dotnet');a=p.parse_args();root=Path(__file__).resolve().parent
 def verified(rel):
  data=(a.source/rel).read_bytes();official=urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{rel}',timeout=30).read()
  if data!=official:raise ValueError('wrong source '+rel)
  return rel,data
 with ThreadPoolExecutor(max_workers=4) as pool:sources=dict(pool.map(verified,FILES))
 text={p:b.decode('utf-8-sig') for p,b in sources.items()}
 enchant=text[NET+'Structure/Enchantment.cs'];registry=text[NET+'Structure/EnchantmentRegistry.cs'];layer=text[NET+'Structure/LayeredSpell.cs']
 methods='\n'.join([method(enchant,'public static void Write(this BinaryWriter writer, Enchantment enchantment)'),method(enchant,'public static void Write(this BinaryWriter writer, List<Enchantment> enchantments)'),method(registry,'public static void Write(this BinaryWriter writer, EnchantmentRegistry registry)'),method(layer,'public static void Write(this BinaryWriter writer, List<LayeredSpell> spells)'),method(layer,'public static void Write(this BinaryWriter writer, LayeredSpell spell)'),method(layer,'public static void Write(this BinaryWriter writer, PropertiesEnchantmentRegistry enchantment)'),method(layer,'public static void Write(this BinaryWriter writer, List<PropertiesEnchantmentRegistry> enchantments)')])
 harness=(root/'magic_harness.cs').read_text().replace('// WRITERS',methods).replace('// MANA',method(text['Source/ACE.Server/WorldObjects/Creature_Magic.cs'],'public static uint GetManaCost('))
 harness=harness.replace('// BURN',method(text['Source/ACE.Server/Entity/Spell.cs'],'public List<uint> TryBurnComponents('))
 enum=method(text['Source/ACE.Entity/Enum/SpellId.cs'],'public enum SpellId : uint')
 with tempfile.TemporaryDirectory(prefix='ace-magic-oracle-') as td:
  b=Path(td);(b/'Program.cs').write_text(harness);(b/'SpellId.cs').write_text('namespace ACE.Entity.Enum {\n'+enum+'\n}')
  for i,rel in enumerate([NET+'GameEvent/GameEventType.cs',NET+'GameMessageGroup.cs','Source/ACE.Entity/Enum/EnchantmentCategory.cs','Source/ACE.Server/WorldObjects/SkillCheck.cs']+[NET+'GameEvent/Events/GameEvent'+name+'.cs' for name in EVENTS]):(b/f'{i}.cs').write_bytes(sources[rel])
  (b/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>')
  subprocess.run([a.dotnet,'build','--nologo','-o',str(b/'out'),str(b/'Oracle.csproj')],check=True)
  vectors=json.loads(subprocess.check_output([a.dotnet,str(b/'out/Oracle.dll')],text=True))
 fixture={'repository':'https://github.com/ACEmulator/ACE','commit':PIN,'source_sha256':{p:hashlib.sha256(d).hexdigest() for p,d in sources.items()},'harness':'Original event wrappers and SkillCheck; verbatim enchantment/registry/layer writers and GetManaCost; typed input/session/random adapters, no tested calculation is replaced.','vectors':vectors}
 (root.parent/'fixtures/magic.json').write_text(json.dumps(fixture,indent=2)+'\n')
if __name__=='__main__':main()
