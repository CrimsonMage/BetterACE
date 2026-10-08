#!/usr/bin/env python3
"""Pinned verbatim ACE specialization consumer vectors with synthetic adapters."""
import argparse,hashlib,subprocess,tempfile,urllib.request
from pathlib import Path
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args()
def method(s,signature):
 start=s.index(signature);opening=s.index('{',start);depth=0
 for i in range(opening,len(s)):
  depth+=(s[i]=='{')-(s[i]=='}')
  if depth==0:return s[start:i+1]
 raise ValueError(signature)
files={};headers=[f'# official ACE {PIN}; synthetic scalar specialization fixture; AGPL-3.0-only']
for name in ['Creature_Rating.cs','Creature_Combat.cs','Player_Combat.cs','SpellProjectile.cs','Healer.cs','SkillFormula.cs']:
 rel='Source/ACE.Server/WorldObjects/'+name;data=(a.source/rel).read_bytes()
 if data!=urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{rel}',timeout=30).read():raise ValueError(rel)
 files[name]=data.decode('utf-8-sig');headers.append(f'# sha256 {hashlib.sha256(data).hexdigest()} {rel}')
rel='Source/ACE.Entity/Enum/SpellId.cs';data=(a.source/rel).read_bytes()
if data!=urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{rel}',timeout=30).read():raise ValueError(rel)
headers.append(f'# sha256 {hashlib.sha256(data).hexdigest()} {rel}')
source=(Path(__file__).parent/'specialization_harness.cs').read_text().replace('// SPELL_ENUM',method(data.decode('utf-8-sig'),'public enum SpellId'))
for marker,name,signature in [('ARMOR','SkillFormula.cs','public static float CalcArmorMod('),('RATING','Creature_Rating.cs','public static float GetNegativeRatingMod('),('POSITIVE','Creature_Rating.cs','public static float GetPositiveRatingMod('),('DEFENSE','Creature_Rating.cs','public int GetSpecDefenseBonus('),('SNEAK','Creature_Combat.cs','public float GetSneakAttackMod('),('RECK','Player_Combat.cs','public float GetRecklessnessMod('),('SHIELD','SpellProjectile.cs','public float GetShieldMod('),('HEAL','Healer.cs','public bool DoSkillCheck(')]:source=source.replace('// '+marker,method(files[name],signature))
with tempfile.TemporaryDirectory(prefix='bace-spec-oracle-') as td:
 b=Path(td);(b/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>');(b/'Program.cs').write_text(source)
 subprocess.run([a.dotnet,'build','--nologo','-o',str(b/'out'),str(b/'Oracle.csproj')],check=True)
 output=subprocess.check_output([a.dotnet,str(b/'out/Oracle.dll')],text=True)
(Path(__file__).parent.parent/'tests/fixtures/specialization.csv').write_text('\n'.join(headers)+'\n'+output)
