"""Independent compiled GDLE methods. Local checkout verified against pinned git objects."""
import argparse,hashlib,subprocess,tempfile
from pathlib import Path
PIN='353cbab52ef7da2b7063bc3e3f008461d8531693'
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);a=p.parse_args()
headers=[f'# GDLE {PIN}; unchanged source methods, synthetic quality/CMT adapters; AGPL-3.0-only']
def read(rel):
 data=(a.source/rel).read_bytes();original=subprocess.check_output(['git','-C',str(a.source),'show',f'{PIN}:{rel}']);assert data==original,rel
 headers.append(f'# sha256 {hashlib.sha256(data).hexdigest()} {rel}');return data.decode('utf-8-sig')
def method(s,sig):
 start=s.index(sig);op=s.index('{',start);depth=0
 for i in range(op,len(s)):
  depth+=(s[i]=='{')-(s[i]=='}')
  if depth==0:return s[start:i+1]
 raise ValueError(sig)
m=read('Source/combat/MeleeAttackEventData.cpp');d=read('Source/combat/DualWieldAttackEventData.cpp');enums=read('Source/PhatSDK/GameEnums.h');f=read('Source/PhatSDK/Support/CombatFormulas.cpp');u=read('Source/Util.cpp');s=read('Source/PhatSDK/Support/SkillChecks.cpp')
h=Path(__file__).with_name('physical_harness.cpp').read_text()
for marker,value in [('ATTACK_ENUM',method(enums,'enum AttackType')+';'),('SETUP',method(m,'void CMeleeAttackEvent::Setup()')),('DUAL_SETUP',method(d,'void CDualWieldAttackEvent::Setup()')),('IMBUE',method(f,'double GetImbueMultiplier(')),('RATING',method(u,'float GetRatingMod(')),('SKILL',method(s,'double GetSkillChance('))]:h=h.replace('// '+marker,value)
with tempfile.TemporaryDirectory(prefix='bace-gdle-physical-')as td:
 b=Path(td);(b/'oracle.cpp').write_text(h);subprocess.run(['c++','-std=c++17','-O0','-ffp-contract=off',str(b/'oracle.cpp'),'-o',str(b/'oracle')],check=True);out=subprocess.check_output([str(b/'oracle')],text=True)
Path(__file__).parent.parent.joinpath('tests/fixtures/physical.csv').write_text('\n'.join(headers)+'\n'+out)
