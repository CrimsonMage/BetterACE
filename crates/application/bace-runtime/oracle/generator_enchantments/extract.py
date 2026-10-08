#!/usr/bin/env python3
"""Extract unchanged pinned ACE routing switch and HasItemCategory property."""
from pathlib import Path
import hashlib,sys
root=Path(sys.argv[1]);out=Path(__file__).parent
paths=['Source/ACE.Server/WorldObjects/Creature_Magic.cs','Source/ACE.Server/Entity/Spell.cs','Source/ACE.Entity/Enum/SpellCategory.cs','Source/ACE.Entity/Enum/MagicSchool.cs']
def block(text, start):
    at=text.index(start); brace=text.index('{',at); level=1;end=brace+1
    while level:
        if text[end]=='{':level+=1
        elif text[end]=='}':level-=1
        end+=1
    return text[at:end]
creature=(root/paths[0]).read_text();creature=creature[creature.index('public bool CreateItemSpell'):]
switch=block(creature,'switch (spell.School)')
prop=block((root/paths[1]).read_text(),'public bool HasItemCategory')
(out/'Routing.cs').write_text('// Official ACEmulator/ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b; AGPL-3.0-only.\nusing ACE.Entity.Enum;\nclass Spell { public MagicSchool School; public SpellCategory Category; public bool IsPortalSpell;\n'+prop+'\n}\nclass Creature {public int Result; void HandleCastSpell(Spell spell, object target, object caster, object? weapon=null,bool equip=false){Result=ReferenceEquals(target,this)?1:2;} public int Route(Spell spell,object item) { Result=0;\n'+switch+'\nreturn Result;} }\n')
(out/'SpellCategory.cs').write_bytes((root/paths[2]).read_bytes())
(out/'MagicSchool.cs').write_bytes((root/paths[3]).read_bytes())
(out/'source.sha256').write_text(''.join(hashlib.sha256((root/p).read_bytes()).hexdigest()+'  '+p+'\n' for p in paths))
