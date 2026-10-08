#!/usr/bin/env python3
from pathlib import Path
import hashlib,shutil
here=Path(__file__).resolve().parent;root=here.parents[4]
pin='47edade3bd3f6044b676d4eb877c4965c7eda62b';source=root/'.reference'/('ACE-'+pin)/'Source'
(here/'extracted').mkdir(exist_ok=True)
paths=[f'ACE.Server/Entity/Mutations/{name}.cs' for name in ['MutationFilter','Mutation','MutationOutcome','EffectList','Effect','EffectArgument','EffectArgumentOp','MutationCache']]
paths+=[f'ACE.Entity/Enum/{name}.cs' for name in ['StatType','EffectArgumentType','MutationEffectType','WieldRequirement']]
hashes=[]
for name in paths:
 src=source/name;shutil.copyfile(src,here/'extracted'/src.name);hashes.append(f'{hashlib.sha256(src.read_bytes()).hexdigest()}  Source/{name}')
(here/'source.sha256').write_text('\n'.join(hashes)+'\n')
resources=[]
for src in sorted((here/'scripts').rglob('*.txt')):
 name=str(src.relative_to(here));logical='ACE.Server.Entity.Mutations.'+str(src.relative_to(here/'scripts')).replace('/','.')
 resources.append(f'<EmbeddedResource Include="{name}" LogicalName="{logical}" />')
(here/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings><Nullable>disable</Nullable><NoWarn>CS0219</NoWarn></PropertyGroup><ItemGroup>'+'\n'.join(resources)+'</ItemGroup></Project>\n')
