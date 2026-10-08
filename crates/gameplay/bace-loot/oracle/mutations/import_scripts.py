#!/usr/bin/env python3
"""Import exact licensed official ACE compatibility scripts; no DAT inputs."""
from pathlib import Path
import hashlib,shutil
here=Path(__file__).resolve().parent
root=here.parents[4]
pin='47edade3bd3f6044b676d4eb877c4965c7eda62b'
source=root/'.reference'/('ACE-'+pin)/'Source/ACE.Server/Entity/Mutations'
rows=[];hashes=[]
for group in ['MeleeWeapons','MissileWeapons','Casters','ArmorLevel']:
 for src in sorted((source/group).rglob('*.txt')):
  name=str(src.relative_to(source));dest=here/'scripts'/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(src,dest)
  rows.append(f'    ("{name}", include_str!("../../oracle/mutations/scripts/{name}")),')
  hashes.append(f'{hashlib.sha256(src.read_bytes()).hexdigest()}  {name}')
(root/'crates/gameplay/bace-loot/src/treasure_mutations/pinned.rs').write_text('// Imported from official ACEmulator/ACE '+pin+'; AGPL-3.0-only.\n// Regenerate with oracle/mutations/import_scripts.py.\npub(super) const SCRIPTS: &[(&str, &str)] = &[\n'+'\n'.join(rows)+'\n];\n')
(here/'scripts.sha256').write_text('\n'.join(hashes)+'\n')
