#!/usr/bin/env python3
"""Run unmodified pinned ACE periodic damage and debuff methods with synthetic owners."""
import argparse, hashlib, subprocess, tempfile
from pathlib import Path
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args()
def method(s, signature):
 start=s.index(signature);opening=s.index('{',start);depth=0
 for i in range(opening,len(s)):
  depth+=(s[i]=='{')-(s[i]=='}')
  if depth==0:return s[start:i+1]
 raise ValueError(signature)
rel='Source/ACE.Server/WorldObjects/Managers/EnchantmentManager.cs';data=(a.source/rel).read_bytes();manager=data.decode('utf-8-sig')
source=(Path(__file__).parent/'dirty_harness.cs').read_text()
for marker,signature in [('DAMAGE','public void ApplyDamageTick('),('ADD','public float GetAdditiveMod(List<PropertiesEnchantmentRegistry>'),('DEFENSE','public int GetDefenseDebuffMod('),('ATTACK','public int GetAttackDebuffMod(')]: source=source.replace('// '+marker,method(manager,signature))
rel2='Source/ACE.Server/WorldObjects/Creature_Rating.cs';rating=(a.source/rel2).read_bytes();text=rating.decode('utf-8-sig')
rel3='Source/ACE.Entity/Models/PropertiesEnchantmentRegistryExtensions.cs';registry=(a.source/rel3).read_bytes()
source=source.replace('// TOP',method(registry.decode('utf-8-sig'),'public static List<PropertiesEnchantmentRegistry> GetEnchantmentsTopLayer('))
source=source.replace('// NEGATIVE',method(text,'public static float GetNegativeRatingMod(')).replace('// POSITIVE',method(text,'public static float GetPositiveRatingMod('))
with tempfile.TemporaryDirectory(prefix='bace-dirty-oracle-') as tmp:
 b=Path(tmp);(b/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><RollForward>Major</RollForward></PropertyGroup></Project>');(b/'Program.cs').write_text(source)
 subprocess.run([a.dotnet,'build','--nologo','-o',str(b/'out'),str(b/'Oracle.csproj')],check=True)
 output=subprocess.check_output([a.dotnet,str(b/'out/Oracle.dll')],text=True)
(Path(__file__).parent.parent/'tests/fixtures/dirty.csv').write_text(f'# official ACE {PIN}; AGPL-3.0-only; original methods, synthetic world adapters\n# sha256 {hashlib.sha256(data).hexdigest()} {rel}\n# sha256 {hashlib.sha256(rating).hexdigest()} {rel2}\n# sha256 {hashlib.sha256(registry).hexdigest()} {rel3}\n'+output)
