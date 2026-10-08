"""Generate language metadata from pinned EmoteScript and numeric IDs from ACE."""
import hashlib
import re
from pathlib import Path
root=Path(__file__).resolve().parents[4]
source=root/'.reference/EmoteScript/EmoteScriptLib'
ace=root/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Entity/Enum'
files={}
def read(p):
    data=p.read_bytes();files[str(p.relative_to(root))]=hashlib.sha256(data).hexdigest();return data.decode('utf-8-sig')
rows=['# EmoteScript aa22635cecc32f534d3630882e96dada38dd6f6d (LGPL-3.0); numeric IDs: pinned ACE (AGPL-3.0-only).']
for kind,name,field in [('action','Emote_Line','EmoteField'),('set','EmoteSet_Line','EmoteSetField')]:
    text=read(source/(name+'.cs'))
    for name,typ in re.findall(r'\{ '+field+r'\.(\w+),\s+FieldType\.(\w+) \}',text): rows.append('\t'.join(['field',kind,name,typ]))
    text=text.split('DefaultFields =',1)[1]
    for name,fields in re.findall(r'\{ Emote(?:Type|Category)\.(\w+), new List<\w+>\(\) \{([^}]+)\}',text):
        rows.append('\t'.join(['default',kind,name,','.join(re.findall(field+r'\.(\w+)',fields))]))
branches={name:re.findall(r'EmoteCategory\.(\w+)',body) for name,body in re.findall(r'List<EmoteCategory> (\w+).*?\{(.*?)\};',read(source/'Branch.cs'),re.S)}
for p in sorted((source/'Emotes').glob('*.cs')):
    text=read(p)
    names=re.findall(r'AddValidBranches\(Branch\.(\w+)\)',text)
    if names: rows.append('\t'.join(['branch','action',p.stem,','.join(n for b in names for n in branches[b])]))
for family in ['EmoteCategory','EmoteType','SpellId','PaletteTemplate','CharacterTitle','ContractId']:
    n=-1
    for name,value in re.findall(r'^\s*(\w+)\s*(?:=\s*(0x[0-9A-Fa-f]+|\d+))?\s*,?\s*(?://.*)?$',read(ace/(family+'.cs')),re.M):
        if name in ['namespace','public']: continue
        n=int(value,0) if value else n+1
        rows.append('\t'.join(['enum',family,name,str(n)]))
output=Path(__file__).with_name('emotescript-schema.tsv');output.write_text('\n'.join(rows)+'\n')
output.with_suffix('.provenance.toml').write_text('reference = "https://github.com/ACEmulator/EmoteScript"\ncommit = "aa22635cecc32f534d3630882e96dada38dd6f6d"\nlicense = "LGPL-3.0"\n\n[source_sha256]\n'+''.join(f'"{k}" = "{v}"\n' for k,v in files.items()))
