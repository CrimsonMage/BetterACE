#!/usr/bin/env python3
"""Extract unchanged pinned ACE methods into an independent compiled oracle."""
from pathlib import Path
import hashlib, subprocess
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
PATH='Source/ACE.Server/WorldObjects/WorldObject_Generators.cs'
p=ROOT/'.reference'/('ACE-'+PIN)/PATH
source=p.read_text() if p.exists() else subprocess.check_output(['git','-C',str(ROOT.parent/'ACE'),'show',f'{PIN}:{PATH}'],text=True)
def method(signature):
 start=source.index('        '+signature)
 brace=source.index('{',start);depth=1;end=brace+1
 while depth:
  depth+=(source[end]=='{')-(source[end]=='}');end+=1
 return source[start:end]
methods=['public void SelectAProfile()','public float GetTotalProbability()','public float GetAdjustedProbability(int index)','public int GetSpawnObjectsForProfile(GeneratorProfile profile)','public bool GenStopSelectProfileConditions']
text='// ACEmulator/ACE '+PIN+'; AGPL-3.0-only. Extracted unchanged methods.\n'
text+='// Source '+PATH+' SHA256 '+hashlib.sha256(source.encode()).hexdigest()+'\n'
text+='using System;\npublic partial class WorldObject {\n'+'\n'.join(map(method,methods))+'\n}\n'
(HERE/'Extracted.cs').write_text(text)

PATH='Source/ACE.Server/Entity/GeneratorProfile.cs'
p=ROOT/'.reference'/('ACE-'+PIN)/PATH
source=p.read_text() if p.exists() else subprocess.check_output(['git','-C',str(ROOT.parent/'ACE'),'show',f'{PIN}:{PATH}'],text=True)
text='// ACEmulator/ACE '+PIN+'; AGPL-3.0-only. Extracted unchanged methods.\n'
text+='// Source '+PATH+' SHA256 '+hashlib.sha256(source.encode()).hexdigest()+'\n'
text+='using System;\nusing System.Numerics;\npublic partial class GeneratorProfile {\n'+method('public bool Spawn_Specific(WorldObject obj)')+'\n}\n'
(HERE/'ExtractedPlacement.cs').write_text(text)
PATH='Source/ACE.Server/WorldObjects/WorldObject_Generators.cs'
p=ROOT/'.reference'/('ACE-'+PIN)/PATH
source=p.read_text() if p.exists() else subprocess.check_output(['git','-C',str(ROOT.parent/'ACE'),'show',f'{PIN}:{PATH}'],text=True)
signatures=['public void CheckGeneratorStatus()','public void CheckTimeOfDayStatus()','public void CheckRealTimeStatus()','public void CheckEventStatus()','public void HandleStatusStaged(bool prevDisabled, bool cond1, bool cond2)','public void Generator_Update()']
text='// ACEmulator/ACE '+PIN+'; AGPL-3.0-only. Unchanged status methods.\n'
text+='// Source '+PATH+' SHA256 '+hashlib.sha256(source.encode()).hexdigest()+'\n'
text+='using System;\npublic partial class WorldObject {private bool eventStatusChanged;\n'+'\n'.join(map(method,signatures))+'\n}\n'
(HERE/'ExtractedStatus.cs').write_text(text)
