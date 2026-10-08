#!/usr/bin/env python3
"""Original GDLE complete projectile scalar damage chain and mitigation prefix.
Collision admission, messages, cloak/aetheria procs and later vital mutations are
outside this numerical fixture; their omission is explicit, never stub success.
"""
import argparse,hashlib,re,subprocess,tempfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);a=p.parse_args();pin='353cbab52ef7da2b7063bc3e3f008461d8531693';sources={}
def read(rel):
 d=(a.source/rel).read_bytes();assert d==subprocess.check_output(['git','-C',str(a.source),'show',f'{pin}:{rel}']);sources[rel]=d;return d.decode('utf-8-sig')
def method(s,n):
 a=s.index(n);op=s.index('{',a);depth=0
 for b in range(op,len(s)):
  depth+=(s[b]=='{')-(s[b]=='}')
  if depth==0:return s[a:b+1]
 raise ValueError(n)
f=read('Source/PhatSDK/Support/CombatFormulas.cpp');w=read('Source/WeenieObject.cpp');h=read('Source/WeenieObject.h');q=read('Source/PhatSDK/Qualities.h');p=read('Source/SpellProjectile.cpp');u=read('Source/Util.cpp');ms=read('Source/PhatSDK/MagicSystem.cpp');read('Source/PhatSDK/SpellTable.cpp')
methods=[method(f,n) for n in ['double GetImbueMultiplier(','void CalculateDamage(','void CalculateAttributeDamageBonus(','void CalculateSkillDamageBonus(','void CalculateCriticalHitData(','void CalculateSlayerData(','void CalculateRendingAndMiscData(','void CalculateRatingData(','void CalculateAttackConditions(']]
damage=w[w.index('void CWeenieObject::TakeDamage('):];damage=damage[:damage.index('\n\tif (_IsPlayer())')]+ '\n}'
# Full original aggregate declarations preserve default initialization/order.
dmg=method(h,'struct DamageEventData')+';';qual=method(q,'struct EnchantedQualityDetails')+';'
# Quality declaration continues with no engine-owned fields; methods use std::max.
block=p[p.index('\t\t\t\t\tint preVarianceDamage;'):p.index('\n\t\t\t\t\tpHit->TryToDealDamage(dmgEvent);')]
# Callback stores the already-calculated event and invokes original mitigation.
block+='\n last_before=dmgEvent.damageBeforeMitigation;last_crit=dmgEvent.wasCrit;last_defended=dmgEvent.critDefended;last_sneak=dmgEvent.isSneakAttack;\n pHit->TakeDamage(dmgEvent);last_damage=dmgEvent.outputDamageFinal;'
cast=read('Source/SpellcastingManager.cpp');adjust=method(cast,'bool CSpellcastingManager::AdjustVital(')
prefix=adjust[adjust.index('BoostSpellEx *meta ='):adjust.index('// negative spell')]
direct=prefix+adjust[adjust.index('DamageEventData dmgEvent;'):adjust.index('m_pWeenie->TryToDealDamage(dmgEvent);')]
direct+='\nlast_before=dmgEvent.damageBeforeMitigation;last_crit=dmgEvent.wasCrit;last_defended=dmgEvent.critDefended;last_sneak=dmgEvent.isSneakAttack;target->TakeDamage(dmgEvent);last_damage=dmgEvent.outputDamageFinal;'
text='\n'.join(methods)+damage+dmg+qual+block+direct
names=set(re.findall(r'\b[A-Z][A-Z0-9_]+\b',text));names-={'NULL','TRUE','FALSE','F_EPSILON'}
# Stable public numeric values used by fixture inputs; other constants are only
# synthetic quality-map keys and never claimed as wire encoding vectors.
fixed={'DF_UNDEF':0,'DF_MELEE':1,'DF_MISSILE':2,'DF_MAGIC':4,'DF_PHYSICAL':3,'UNDEF_SKILL':0,'WAR_MAGIC_SKILL':34,'VOID_MAGIC_SKILL':43,'MAGIC_DEFENSE_SKILL':15,'MELEE_DEFENSE_SKILL':6,'MISSILE_DEFENSE_SKILL':7,'SNEAK_ATTACK_SKILL':51,'DECEPTION_SKILL':20,'PERSONAL_APPRAISAL_SKILL':19,'SHIELD_SKILL':48,'RECKLESSNESS_SKILL':50,'SLASH_DAMAGE_TYPE':1,'PIERCE_DAMAGE_TYPE':2,'BLUDGEON_DAMAGE_TYPE':4,'FIRE_DAMAGE_TYPE':8,'COLD_DAMAGE_TYPE':16,'ACID_DAMAGE_TYPE':32,'ELECTRIC_DAMAGE_TYPE':64,'HEALTH_DAMAGE_TYPE':128,'STAMINA_DAMAGE_TYPE':256,'MANA_DAMAGE_TYPE':512,'NETHER_DAMAGE_TYPE':1024,'BASE_DAMAGE_TYPE':0x10000000,'SPECIALIZED_SKILL_ADVANCEMENT_CLASS':3,'TRAINED_SKILL_ADVANCEMENT_CLASS':2,'UNTRAINED_SKILL_ADVANCEMENT_CLASS':1}
# Scoped enum types are defined explicitly below.
names-={'SPECIALIZED_SKILL_ADVANCEMENT_CLASS','TRAINED_SKILL_ADVANCEMENT_CLASS','UNTRAINED_SKILL_ADVANCEMENT_CLASS'}
names-= {'DAMAGE_FORM','DAMAGE_TYPE','DAMAGE_QUADRANT','BODY_PART_ENUM','SKILL_ADVANCEMENT_CLASS','STypeSkill','COMBAT_USE','COMBAT_USE_MISSILE','COMBAT_USE_SHIELD','DQ_UNDEF','BP_UNDEF'}
constants='enum {'+','.join(f'{name}={fixed.get(name,10000+i)}'for i,name in enumerate(sorted(names)))+'};'
program=Path(__file__).with_name('damage_harness.cpp').read_text()
for key,value in [('CONSTANTS',constants),('QUALITY',qual),('DAMAGE_EVENT',dmg),('METHOD_DECLS','\n'.join(m[:m.index('{')].strip()+';'for m in methods)),('METHODS','\n'.join(methods)),('TAKE_DAMAGE',damage),('PROJECTILE',block),('DIRECT',direct),('RATING',method(u,'float GetRatingMod(').replace('float GetRatingMod(', 'float SourceRating(')),('FORMULA',method(ms,'uint32_t MagicSystem::DeterminePowerLevelOfComponent('))]:program=program.replace('// '+key,value)
program=program.replace('bool allowNegative);','bool allowNegative=false);')
with tempfile.TemporaryDirectory(prefix='gdle-damage-') as td:
 b=Path(td);(b/'main.cpp').write_text(program);subprocess.run(['c++','-std=c++17','-O0','-fno-fast-math','-ffp-contract=off',str(b/'main.cpp'),'-o',str(b/'main')],check=True);out=subprocess.check_output([str(b/'main')],text=True)
header=[f'# GDLE {pin}; AGPL-3.0-only, GDLE contributors']+[f'# sha256 {hashlib.sha256(v).hexdigest()} {k}'for k,v in sources.items()]+['# Original projectile damage statements, complete CombatFormulas methods, TakeDamage prefix through rounding; synthetic quality/equipment/RNG adapters. Source negative rating is recorded; production uses separately reviewed correction. Excludes collision admission, procs, messages and vital mutation.']
Path(__file__).parent.parent.joinpath('tests/fixtures/damage.csv').write_text('\n'.join(header)+'\n'+out)
