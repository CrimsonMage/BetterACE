#!/usr/bin/env python3
"""Compile unchanged official ACE methods/lists against minimal deterministic adapters.
AGPL-3.0-only; original methods copyright ACEmulator contributors.
"""
from pathlib import Path
import hashlib, subprocess
ROOT=Path(__file__).resolve().parents[5]
ACE=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b'
OUT=Path(__file__).resolve().parent
assert ACE.is_dir(), 'pinned source snapshot required'
files={}
def text(name):
 p=ACE/'Source'/name;files[name]=hashlib.sha256(p.read_bytes()).hexdigest();return p.read_text(encoding='utf-8-sig')
def method(s,signature):
 start=s.index(signature);brace=s.index('{',start);depth=1;i=brace+1
 while depth:
  depth+=(s[i]=='{')-(s[i]=='}');i+=1
 return s[start:i]
def field(s,signature):
 start=s.index(signature);end=s.index('};',start)+2;return s[start:end]
death=text('ACE.Server/WorldObjects/Player_Death.cs');combat=text('ACE.Server/WorldObjects/Player_Combat.cs');location=text('ACE.Server/WorldObjects/Player_Location.cs');corpse=text('ACE.Server/WorldObjects/Corpse.cs');creature=text('ACE.Server/WorldObjects/Creature_Death.cs');ench=text('ACE.Server/WorldObjects/Managers/EnchantmentManager.cs');pos=text('ACE.Entity/Position.cs')
source='// Original ACE methods, AGPL-3.0-only, ACEmulator contributors. See source.sha256.\nusing System;using System.Collections.Generic;using ACE.Entity.Enum;\n'
source+='public partial class Player {\n'+method(death,'public int GetNumItemsDropped(Corpse corpse)')+'\n'+method(death,'public int GetNumCoinsDropped()')+'\n'+method(combat,'public bool IsPKDeath(uint? killerGuid)')+'\n'+method(combat,'public bool IsPKLiteDeath(uint? killerGuid)')+'\n'+field(location,'public static HashSet<ushort> NoLog_Landblocks')+'\n}\n'
source+='public partial class Corpse {\n'+field(corpse,'public static HashSet<ushort> NoDrop_Landblocks')+'\n}\n'
source+='public class Creature {\n'+field(creature,'public static HashSet<ushort> NoDeathXP_Landblocks')+'\n}\n'
source+='public class EnchantmentManager {\n'+method(ench,'public float GetMinVitae(uint level)')+'\n}\n'
source+='public partial class Position {\n'+method(pos,'public float SquaredDistanceTo(Position p)')+'\n}\n'
(OUT/'Original.cs').write_text(source)
(OUT/'PlayerKillerStatus.cs').write_text(text('ACE.Entity/Enum/PlayerKillerStatus.cs'))
(OUT/'source.sha256').write_text(''.join(f'{v}  {k}\n' for k,v in sorted(files.items())))
