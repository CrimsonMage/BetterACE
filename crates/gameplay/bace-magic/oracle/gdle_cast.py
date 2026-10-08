#!/usr/bin/env python3
"""Compile verbatim GDLE cast-control methods; world/animation/effect adapters only."""
import argparse,hashlib,subprocess,tempfile
from pathlib import Path
PIN='353cbab52ef7da2b7063bc3e3f008461d8531693'
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);a=p.parse_args();root=Path(__file__).resolve().parent
sources={}
for rel in ['Source/SpellcastingManager.cpp','Source/PhatSDK/GameEnums.h','Source/WeenieObject.cpp']:
 data=(a.source/rel).read_bytes()
 if data!=subprocess.check_output(['git','-C',str(a.source),'show',f'{PIN}:{rel}']):raise ValueError('wrong pinned reference '+rel)
 sources[rel]=data

def method(s,signature):
 start=s.index(signature);opening=s.index('{',start);depth=0
 for i in range(opening,len(s)):
  depth+=(s[i]=='{')-(s[i]=='}')
  if depth==0:return s[start:i+1]
 raise ValueError(signature)
s=sources['Source/SpellcastingManager.cpp'].decode('utf-8-sig')
methods='\n'.join(method(s,sig)for sig in ['void CSpellcastingManager::EndCast(','bool CSpellcastingManager::MotionRequiresHeading(','void CSpellcastingManager::BeginNextMotion(','void CSpellcastingManager::Update(','void CSpellcastingManager::HandleMotionDone(', 'int CSpellcastingManager::CastSpellInstant('])
methods+='\n'+method(sources['Source/WeenieObject.cpp'].decode('utf-8-sig'),'bool CWeenieObject::IsInPeaceMode(')+'\n'+method(sources['Source/WeenieObject.cpp'].decode('utf-8-sig'),'int CWeenieObject::AdjustMana(')
constants='\n'.join(line for line in s.splitlines()if line.startswith('const float MAX_')or line.startswith('const float CAST_UPDATE_RATE'))
enum=method(sources['Source/PhatSDK/GameEnums.h'].decode('utf-8-sig'),'enum WErrorType')+';'
harness=(root/'gdle_cast.cpp').read_text().replace('// METHODS',methods).replace('// CONSTANTS',constants).replace('// ERRORS',enum)
with tempfile.TemporaryDirectory(prefix='gdle-cast-oracle-')as td:
 b=Path(td);(b/'cast.cpp').write_text(harness);subprocess.run(['c++','-std=c++17','-O0','-fno-fast-math','-ffp-contract=off',str(b/'cast.cpp'),'-o',str(b/'cast')],check=True)
 output=subprocess.check_output([str(b/'cast')],text=True)
 headers=[f'# GDLE {PIN}; AGPL-3.0-only, GDLE contributors']+[f'# sha256 {hashlib.sha256(data).hexdigest()} {name}'for name,data in sources.items()]+['# Verbatim EndCast/MotionRequiresHeading/BeginNextMotion/Update/HandleMotionDone/CastSpellInstant; IsInPeaceMode and AdjustMana verbatim too; synthetic accepted heading/distance/style and motion/effect adapters; not full spell effects.']
 out=root.parent/'tests/fixtures/gdle_cast.csv';out.parent.mkdir(parents=True,exist_ok=True);out.write_text('\n'.join(headers)+'\n'+output)
