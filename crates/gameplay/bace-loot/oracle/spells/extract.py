#!/usr/bin/env python3
from pathlib import Path
import hashlib,shutil
here=Path(__file__).resolve().parent;root=here.parents[4];pin='47edade3bd3f6044b676d4eb877c4965c7eda62b';src=root/'.reference'/('ACE-'+pin)/'Source'
paths=['ACE.Server/Factories/LootGenerationFactory_PetDevice.cs','ACE.Server/Factories/LootGenerationFactory_Aetheria.cs','ACE.Server/Factories/Tables/AetheriaChance.cs','ACE.Server/Factories/Tables/Wcids/AetheriaWcids.cs','ACE.Server/Factories/Tables/Wcids/CoalescedManaWcids.cs','ACE.Server/Factories/LootGenerationFactory_Magic.cs','ACE.Server/Factories/Tables/WorkmanshipChance.cs','ACE.Server/Factories/LootGenerationFactory_Spells.cs','ACE.Server/Factories/Entity/ChanceTable.cs','ACE.Server/Factories/Entity/TreasureRoll.cs','ACE.Server/Factories/Tables/SpellLevelProgression.cs','ACE.Server/Factories/Tables/SpellLevelChance.cs','ACE.Server/Factories/Tables/SpellSelectionTable.cs']
paths += [f'ACE.Server/Factories/Tables/Spells/{n}Spells.cs' for n in ['Armor','Melee','Missile','Wand']]
paths += [f'ACE.Server/Factories/Tables/Cantrips/{n}Cantrips.cs' for n in ['Armor','Melee','Missile','Wand','Jewelry']]
paths += ['ACE.Server/Factories/Tables/Cantrips/CantripChance.cs']
paths += [f'ACE.Server/Factories/Enum/{n}.cs' for n in ['TreasureItemType','TreasureArmorType','TreasureWeaponType']]
hashes=[]
for path in paths:
 file=src/path;shutil.copyfile(file,here/'extracted'/file.name);hashes.append(f'{hashlib.sha256(file.read_bytes()).hexdigest()}  Source/{path}')
for name in ['SpellId','WieldRequirement','Skill','DamageType','CoverageMask','UiEffects']:
 path=f'ACE.Entity/Enum/{name}.cs';file=src/path;source=file.read_text();a=source.index('    public enum '+name);start=source.index('{',a);depth=1;end=start+1
 while depth:
  depth+=(source[end]=='{')-(source[end]=='}');end+=1
 (here/'extracted'/file.name).write_text('// Verbatim enum extracted from official ACE '+pin+'\nnamespace ACE.Entity.Enum {\n'+source[a:end]+'\n}\n');hashes.append(f'{hashlib.sha256(file.read_bytes()).hexdigest()}  Source/{path} (enum only)')
(here/'source.sha256').write_text('\n'.join(hashes)+'\n')

path='ACE.Server/Factories/LootGenerationFactory.cs';file=src/path;source=file.read_text();pieces=[]
for signature in ['private static WorldObject TryRollMundaneAddon','private static WorldObject TryRollCoalescedMana','private static WorldObject TryRollAetheria']:
 a=source.index('        '+signature);start=source.index('{',a);depth=1;end=start+1
 while depth:
  depth+=(source[end]=='{')-(source[end]=='}');end+=1
 pieces.append(source[a:end])
(here/'extracted/MundaneAddons.cs').write_text('// Unchanged official ACE '+pin+' methods; AGPL-3.0-only.\nusing ACE.Common;using ACE.Database.Models.World;using ACE.Server.WorldObjects;using ACE.Server.Managers;namespace ACE.Server.Factories {public static partial class LootGenerationFactory {\n'+'\n'.join(pieces)+'\n}}\n')
with (here/'source.sha256').open('a') as output:output.write(f'{hashlib.sha256(file.read_bytes()).hexdigest()}  Source/{path} (TryRollMundaneAddon/CoalescedMana/Aetheria only)\n')
