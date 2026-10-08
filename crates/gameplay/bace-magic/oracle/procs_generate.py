#!/usr/bin/env python3
"""Compiled-original GDLE cloak and Aetheria selection; cast calls recorded only."""
import argparse,subprocess,tempfile,hashlib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);a=p.parse_args();pin='353cbab52ef7da2b7063bc3e3f008461d8531693';sources={}
def read(rel):
 d=(a.source/rel).read_bytes();assert d==subprocess.check_output(['git','-C',str(a.source),'show',f'{pin}:{rel}']);sources[rel]=d;return d.decode('utf-8-sig')
def method(s,name):
 start=s.index(name);op=s.index('{',start);depth=0
 for end in range(op,len(s)):
  depth+=(s[end]=='{')-(s[end]=='}')
  if depth==0:return s[start:end+1]
 raise ValueError(name)
game_enums=read('Source/PhatSDK/GameEnums.h');start=game_enums.index('enum SchoolOfMagic');school_enum=game_enums[start:game_enums.index('};',start)+2]
projectile=read('Source/SpellProjectile.cpp');visual=next(line.strip() for line in projectile.splitlines() if 'm_fEffectMod = max(' in line)
player=read('Source/Player.cpp');world=read('Source/WeenieObject.cpp');read('Source/Config.h')
qualities=read('Source/PhatSDK/Qualities.cpp');enums=read('Source/PhatSDK/GameStatEnums.h');start=enums.index('enum STypeAttribute2nd');attribute_enum=enums[start:enums.index('};',start)+2]
getter=method(qualities,'BOOL AttributeCache::InqAttribute2nd(STypeAttribute2nd key, uint32_t &value)')
take=world[world.index('void CWeenieObject::TakeDamage('):];start=take.index('\n\tif (_IsPlayer())');end=take.index('\n\tSTypeAttribute2nd vitalAffected;',start);cloak=take[start:end]
methods='\n'.join(method(player,n)for n in ['std::map<int, double> CPlayerWeenie::GetSigilProcRate(', 'void CPlayerWeenie::HandleAetheriaProc('])
program=Path(__file__).with_name('procs_harness.cpp').read_text().replace('// METHODS',methods).replace('// CLOAK',cloak).replace('// ATTRIBUTE ENUM',attribute_enum).replace('// SCHOOL ENUM',school_enum).replace('// ATTRIBUTE GETTER',getter).replace('// VISUAL', 'for(uint32_t level: {0u,1u,2u,3u,4u,5u,6u,7u,8u,15u,16u,17u,65535u}) { struct { uint32_t power_level_of_power_component; } scd{level}; float m_fEffectMod; '+visual+' std::cout<<"visual,"<<level<<","<<m_fEffectMod<<"\\n"; }')
with tempfile.TemporaryDirectory(prefix='gdle-procs-')as td:
 b=Path(td);(b/'main.cpp').write_text(program);subprocess.run(['c++','-std=c++17','-O0','-fno-fast-math','-ffp-contract=off',str(b/'main.cpp'),'-o',str(b/'main')],check=True);out=subprocess.check_output([str(b/'main')],text=True)
header=[f'# GDLE {pin}; AGPL-3.0-only, GDLE contributors']+[f'# sha256 {hashlib.sha256(v).hexdigest()} {k}'for k,v in sources.items()]+['# Original GetSigilProcRate/HandleAetheriaProc and full TakeDamage cloak branch; explicit item/clockless RNG adapters record requested casts, never claim successful execution.']
Path(__file__).parent.parent.joinpath('tests/fixtures/procs.csv').write_text('\n'.join(header)+'\n'+out)
