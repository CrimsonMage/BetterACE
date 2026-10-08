#!/usr/bin/env python3
"""Extract pinned ACE metadata, retaining duplicate source declarations and hashes.
ACE attribution: ACEmulator contributors, AGPL-3.0-only, pin below.
"""
from pathlib import Path
import collections,hashlib,json,re
ROOT=Path(__file__).resolve().parents[4]
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
SRC=ROOT/'.reference'/('ACE-'+PIN)/'Source/ACE.Server'
OUT=Path(__file__).resolve().parents[1]/'src/command_catalog'
def clean(s):
 return re.sub(r'(@?"(?:\\.|""|[^"\\])*"|\'[^\']*\')|//[^\n]*|/\*.*?\*/',lambda m:m.group(1) or ' '*len(m[0]),s,flags=re.S)
def rust(s):return json.dumps(s,ensure_ascii=False)
def cs_string(s):
 s=s.strip()
 if s.startswith('@"'):return s[2:-1].replace('""','"')
 return "".join(json.loads(m[0]) for m in re.finditer(r'"(?:\\.|[^"\\])*"',s))
def parts(s):
 return [p.strip() for p in re.split(r',(?=(?:[^"\\]*"(?:\\.|[^"\\])*"?)*[^"\\]*$)',s)]
# Scan balanced attributes while respecting C# string tokens.
def attributes(s):
 for m in re.finditer(r'\[CommandHandler\(',s):
  i=m.end();start=i;depth=1;quote=False;escape=False
  while i<len(s):
   c=s[i]
   if quote:
    if escape:escape=False
    elif c=='\\':escape=True
    elif c=='"':quote=False
   elif c=='"':quote=True
   elif c=='(':depth+=1
   elif c==')':
    depth-=1
    if depth==0:yield m.start(),i+2,s[start:i];break
   i+=1
rows=[]
for f in sorted(SRC.rglob('*.cs')):
 raw=f.read_text(encoding='utf-8-sig');s=clean(raw)
 for start,end,text in attributes(s):
  # String-aware comma split, descriptions may contain commas.
  args=[];at=0;quoted=False;escaped=False
  for i,c in enumerate(text):
   if quoted:
    if escaped:escaped=False
    elif c=='\\':escaped=True
    elif c=='"':quoted=False
   elif c=='"':quoted=True
   elif c==',':args.append(text[at:i].strip());at=i+1
  args.append(text[at:].strip())
  name=cs_string(args[0]);access=args[1].split('.')[-1]
  flags='';minimum=-1;include=False;description='';usage='';strings=[]
  for arg in args[2:]:
   if ':' in arg and not arg.startswith('"'):
    key,value=arg.split(':',1);value=value.strip()
    if key=='parameterCount':minimum=int(value)
    elif key=='includeRaw':include=value=='true'
    elif key=='description':description=cs_string(value)
    elif key=='usage':usage=cs_string(value)
    else:raise ValueError(arg)
   elif arg.startswith('CommandHandlerFlag.'):flags=arg
   elif arg in ('true','false'):include=arg=='true'
   elif re.fullmatch(r'-?\d+',arg):minimum=int(arg)
   elif arg.startswith('"'):strings.append(cs_string(arg))
   else:raise ValueError(arg)
  if strings:description=strings[0]
  if len(strings)>1:usage=strings[1]
  method=re.search(r'public static \w+ (\w+)\(',s[end:])
  if not method:raise ValueError(name)
  method_start=end+method.start();brace=s.index('{',method_start);depth=1;i=brace+1
  while depth:
   if s[i]=='{':depth+=1
   elif s[i]=='}':depth-=1
   i+=1
  body=s[brace+1:i-1].strip()
  rows.append(dict(name=name,access=access,console_only='ConsoleInvoke' in flags,requires_world='RequiresWorld' in flags,min_args=minimum,include_raw=include,description=description,usage=usage,handler=method[1],source=str(f.relative_to(SRC)),line=raw[:start].count('\n')+1,source_sha256=hashlib.sha256(f.read_bytes()).hexdigest(),source_stub=not body))
# Source registration overwrites duplicate names. Retain all declarations for
# inspection; the default lookup selects the nonempty implementation, then last.
unique={}
for row in rows:
 old=unique.get(row['name'].lower())
 if old is None or not row['source_stub'] or old['source_stub']:unique[row['name'].lower()]=row
assert len(rows)==324 and len(unique)==321,(len(rows),len(unique))
(OUT.parent.parent/'oracle/commands.json').write_text(json.dumps(dict(pin=PIN,declarations=rows),indent=2)+'\n')
groups=collections.defaultdict(list)
for row in sorted(unique.values(),key=lambda r:r['name']):
 role=row['access'].lower();group=role
 if role=='developer':
  initial=row['name'][0].lower();group+='_'+('a_f' if initial<='f' else 'g_m' if initial<='m' else 'n_s' if initial<='s' else 't_z')
 if role=='admin':group+='_'+('a_m' if row['name'][0].lower()<='m' else 'n_z')
 groups[group].append(row)
mods=[]
for group,entries in sorted(groups.items()):
 out=['//! Generated from pinned official ACE command attributes; AGPL-3.0-only.', 'use super::CommandSpec;', 'use bace_auth::AccessLevel;', 'pub(super) static COMMANDS: &[CommandSpec] = &[']
 for r in entries:
  out.append('    CommandSpec {')
  for k in ['name','handler','source','description','usage']:out.append('        '+k+': '+rust(r[k])+',')
  out.extend(['        access: AccessLevel::'+r['access']+',','        min_args: '+str(r['min_args'])+','])
  for k in ['console_only','requires_world','include_raw','source_stub']:out.append('        '+k+': '+str(r[k]).lower()+',')
  out.append('    },')
 out.append('];');(OUT/(group+'.rs')).write_text('\n'.join(out)+'\n');mods.append(group)
out=['//! Frozen command metadata from official ACE '+PIN+'.', 'use bace_auth::AccessLevel;']
out += ['mod '+m+';' for m in mods]
out += ['#[derive(Clone, Copy, Debug, PartialEq, Eq)]','pub struct CommandSpec {','    pub name: &\'static str,','    pub access: AccessLevel,','    pub min_args: i32,','    pub console_only: bool,','    pub requires_world: bool,','    pub include_raw: bool,','    pub source_stub: bool,','    pub handler: &\'static str,','    pub source: &\'static str,','    pub description: &\'static str,','    pub usage: &\'static str,','}', 'pub fn commands() -> impl Iterator<Item = &\'static CommandSpec> {', '    '+'.chain('.join([]) if False else '']
out=out[:-1]+['    '+mods[0]+'::COMMANDS.iter()']+['        .chain('+m+'::COMMANDS)' for m in mods[1:]]+['}', 'pub fn command(name: &str) -> Option<&\'static CommandSpec> {','    commands().find(|spec| spec.name.eq_ignore_ascii_case(name))','}']
(OUT/'mod.rs').write_text('\n'.join(out)+'\n')
print(len(rows),'declarations',len(unique),'commands',sum(r['source_stub'] for r in unique.values()),'source stubs')
