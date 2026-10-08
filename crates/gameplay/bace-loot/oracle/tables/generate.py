#!/usr/bin/env python3
"""Extract pinned ACE literal treasure data; does not translate roll algorithms.
AGPL-3.0-only; derived data retains ACE contributor attribution and source hashes.
"""
import argparse
import ast
import hashlib
import json
from pathlib import Path
import re
import struct

PIN = '47edade3bd3f6044b676d4eb877c4965c7eda62b'
ROOT = Path(__file__).resolve().parents[5]

def clean(s):
    return re.sub(r'/\*.*?\*/|//[^\n]*', '', s, flags=re.S)

def body(s, start):
    depth = 1
    end = start
    while depth:
        if s[end] == '{': depth += 1
        if s[end] == '}': depth -= 1
        end += 1
    return s[start:end-1]

def extract_enums(source, ace_root):
    """Resolve pinned C# enum members for both oracle and native export."""
    symbols = {'true': 1, 'false': 0}
    unresolved = []
    enum_sources = {}
    for p in source.rglob('*.cs'):
        if '/Enum/' not in str(p) and '/Enums/' not in str(p): continue
        text = clean(p.read_text(encoding='utf-8-sig'))
        for m in re.finditer(r'\benum\s+(\w+)(?:\s*:\s*\w+)?\s*\{', text):
            name = m[1]
            previous = None
            # Attributes may contain commas; remove them before splitting the
            # C# member list (UpdatePositionFlag has SuppressMessage rows).
            members = re.sub(r'\[[^\]]*\]', '', body(text, m.end()))
            for entry in members.split(','):
                entry = entry.strip()
                if not entry: continue
                parts = entry.split('=',1)
                key = name+'.'+parts[0].strip()
                expr = parts[1].strip() if len(parts)>1 else ('0' if previous is None else previous+' + 1')
                unresolved.append((name,key,expr))
                previous = key
            enum_sources[str(p.relative_to(ace_root))] = hashlib.sha256(p.read_bytes()).hexdigest()
    def value(expr, enum=None):
        expr = re.sub(r'\((?:u?int|u?long|byte|short)\)', '', expr)
        expr = re.sub(r'\b(0x[0-9a-fA-F]+|\d+)[uUlL]+\b',r'\1',expr)
        def sub(m):
            token=m[0]
            if token in symbols: return str(symbols[token])
            if enum and enum+'.'+token in symbols: return str(symbols[enum+'.'+token])
            return token
        expr=re.sub(r'\b[A-Za-z_]\w*(?:\.\w+)*\b',sub,expr)
        tree=ast.parse(expr,mode='eval')
        allowed=(ast.Expression,ast.Constant,ast.BinOp,ast.UnaryOp,ast.Add,ast.Sub,ast.Mult,ast.Div,ast.BitOr,ast.BitAnd,ast.BitXor,ast.LShift,ast.RShift,ast.Invert,ast.USub,ast.UAdd)
        if not all(isinstance(n,allowed) for n in ast.walk(tree)): raise ValueError(expr)
        return eval(compile(tree,'<constant>','eval'),{'__builtins__':{}})
    while unresolved:
        pending=[]
        for enum,key,expr in unresolved:
            try: symbols[key]=value(expr,enum)
            except (ValueError,SyntaxError,NameError): pending.append((enum,key,expr))
        if len(pending)==len(unresolved): break
        unresolved=pending
    return symbols, enum_sources, value

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--ace', type=Path, default=ROOT / '.reference' / ('ACE-' + PIN))
    args = parser.parse_args()
    source = args.ace / 'Source'
    symbols, enum_sources, value = extract_enums(source, args.ace)
    chance={}; descriptors={}; typed_references={}; gem={}; floats={}; sequences={}; references={}; float_sequences={}; sources={}; rejected=[]
    declaration=re.compile(r'(?:private|public|internal)\s+static\s+(?:readonly\s+)?(?P<type>ChanceTable<[^;=]+?>|List<[^;=]+?>)\s+(?P<name>\w+)\s*=\s*new\s+[^;{]+\{')
    for p in sorted(list((source/'ACE.Server/Factories/Tables').rglob('*.cs'))+[source/'ACE.Server/Factories/Entity/MissileMagicDefense.cs']):
        text=clean(p.read_text(encoding='utf-8-sig')); cls=re.search(r'\bclass\s+(\w+)',text)[1]
        sources[str(p.relative_to(args.ace))]=hashlib.sha256(p.read_bytes()).hexdigest()
        if cls in ('MaterialTable','CasterSlotSpells'):
            for dm in re.finditer(r'Dictionary<[^>]+>\s+(ValueMod|descriptors)\s*=\s*new[^;{]+\{',text):
                data=body(text,dm.end())
                entries=re.findall(r'\{\s*([\w.]+)\s*,\s*([^}]+)\}',data)
                if dm[1]=='ValueMod':chance[cls+'.'+dm[1]]=[(int(value(a)),float(b.strip().rstrip('f'))) for a,b in entries]
                else:descriptors[cls+'.'+dm[1]]=[(int(value(a)),json.loads(b.strip())) for a,b in entries]
        for m in declaration.finditer(text):
            key=cls+'.'+m['name']; typ=m['type']; data=body(text,m.end())
            try:
                if typ == 'ChanceTable<GemResult>':
                    entries=re.findall(r'\(\s*new GemResult\(([^,]+),\s*([^()]+)\),\s*([^,()]+)\)',data)
                    if not entries: raise ValueError('missing gem tuples')
                    gem[key]=[(int(value(a.strip())),int(value(b.strip())),float(c.strip().rstrip('f'))) for a,b,c in entries]
                elif typ.startswith('ChanceTable<') or typ.startswith('List<(') and 'float' in typ:
                    entries=re.findall(r'\(\s*([^,()]+),\s*([^,()]+)\s*\)',data)
                    if re.sub(r'\(\s*[^,()]+,\s*[^,()]+\s*\)|[\s,]','',data): raise ValueError('complex chance rows')
                    target=floats if typ=='ChanceTable<float>' else chance
                    target[key]=[(float(a.rstrip('f')) if typ=='ChanceTable<float>' else int(value(a.strip())),float(b.strip().rstrip('f'))) for a,b in entries]
                elif typ.startswith('List<('):
                    entries=re.findall(r'\(\s*(\w+)\s*,\s*([\w.]+)\s*\)',data)
                    if not entries:raise ValueError('typed reference list')
                    typed_references[key]=[(cls+'.'+a,int(value(b))) for a,b in entries]
                else:
                    entries=[e.strip() for e in data.split(',') if e.strip()]
                    if 'ChanceTable<' in typ or 'List<' in typ[len('List<'):]:
                        references[key]=[e if '.' in e or e=='null' else cls+'.'+e for e in entries]
                    elif typ=='List<float>': float_sequences[key]=[float(e.rstrip('f')) for e in entries]
                    else: sequences[key]=[int(value(e)) for e in entries]
            except (ValueError,SyntaxError,NameError) as e: rejected.append((key,str(e)))
    # Oracle extraction artifacts must never overwrite the runtime pack loader.
    out=ROOT/'.reference/loot-table-extraction'
    out.mkdir(parents=True, exist_ok=True)
    header='// Generated from official ACEmulator/ACE '+PIN+'.\n// Copyright ACE contributors. AGPL-3.0-only. Regenerate: crates/gameplay/bace-loot/oracle/tables/generate.py\n'
    # Spell progression families are separated by behavior, preserving every row.
    def spell_group(name):
        low=name.lower()
        if low.startswith('cantrip'): return 'cantrips'
        if name.startswith('Set'): return 'equipment_sets'
        if name.startswith('Cloak'): return 'cloaks'
        if any(name.startswith(n) for n in ('Paragon','Gauntlet','Aetheria','Rare','Trinket','Olthoi')): return 'equipment_effects'
        if any(n in name for n in ('Portal','Recall','ReturnTo')): return 'portals'
        skill=any(n in name for n in ('Mastery','Ineptitude','Expertise','Ignorance','Attunement','Unfamiliarity','Enlightenment','Benightedness','Fealty','Faithlessness','Sprint','LeadenFeet','Boon','Prowess'))
        if skill:
            if name.endswith('Self'):return 'skills_self'
            return 'skills_other_debuffs' if any(n in name for n in ('Ineptitude','Ignorance','Unfamiliarity','Benightedness','Faithlessness','LeadenFeet')) else 'skills_other_buffs'
        if name.endswith('Self'): return 'effects_self'
        if name.endswith('Other'): return 'effects_other'
        if any(n in name for n in ('Bolt','Blast','Streak','Volley','Arc','Stream','Blade','Wave','Spit','Firework')): return 'projectiles'
        return 'special'
    original_progression={k:v for k,v in sequences.items() if k.startswith('SpellLevelProgression.')}
    progression_routes={}
    for k,v in original_progression.items():
        field=k.split('.',1)[1]; cls='SpellProgression_'+spell_group(field)
        del sequences[k];sequences[cls+'.'+field]=v;progression_routes[field]=cls
    progression_source=clean((source/'ACE.Server/Factories/Tables/SpellLevelProgression.cs').read_text())
    by_spell={}
    for field in re.findall(r'AddSpells\((\w+)\);',progression_source):
        for spell in original_progression['SpellLevelProgression.'+field]:
            if spell==0:continue
            if spell in by_spell:raise ValueError('duplicate source spell progression')
            by_spell[spell]=field
    pool=bytearray();records=bytearray()
    for spell,field in sorted(by_spell.items()):
        encoded=field.encode();records+=struct.pack('<qII',spell,len(pool),len(encoded));pool+=encoded
    import zlib
    payload=records+pool
    (out/'spell_progression.bace').write_bytes(b'ACEPRG01'+struct.pack('<III',len(by_spell),len(pool),zlib.crc32(payload))+payload)
    modules=[]
    def f32(v):
        bits=struct.unpack('<I',struct.pack('<f',v))[0]
        return f'f32::from_bits(0x{bits:08x})'
    # Each source class is a responsibility boundary, never arbitrary line chunks.
    classes=sorted(set(k.split('.')[0] for table in (chance,gem,typed_references,descriptors,floats,sequences,references,float_sequences) for k in table))
    kinds=[('descriptors',descriptors,"&'static [(i64, &'static str)]",lambda row:f'({row[0]}, {json.dumps(row[1])})'),('typed_references',typed_references,"&'static [(&'static str, i64)]",lambda row:f'({json.dumps(row[0])}, {row[1]})'),('gem',gem,"&'static [(i64, i64, f32)]",lambda row:f'({row[0]}, {row[1]}, {f32(row[2])})'),('lookup',chance,'&\'static [(i64, f32)]',lambda row:f'({row[0]}, {f32(row[1])})'),('lookup_float',floats,'&\'static [(f32, f32)]',lambda row:f'({f32(row[0])}, {f32(row[1])})'),('sequence',sequences,'&\'static [i64]',str),('references',references,'&\'static [&\'static str]',json.dumps),('float_sequence',float_sequences,'&\'static [f32]',f32)]
    for cls in classes:
        module=re.sub('_+','_',re.sub(r'(?<!^)(?=[A-Z])','_',cls).lower());modules.append((cls,module))
        lines=[header]
        for fn,table,ret,render in kinds:
            subset={k:v for k,v in table.items() if k.startswith(cls+'.')}
            if not subset: continue
            lines.append(f'pub(super) fn {fn}(field: &str) -> Option<{ret}> {{\n    match field {{')
            for k,rows in subset.items():
                lines.append(f'        "{k.split(".",1)[1]}" => Some(const {{ &[')
                lines.extend('            '+render(row)+',' for row in rows)
                lines.append('        ] }),')
            lines.append('        _ => None,\n    }\n}')
        (out/(module+'.rs')).write_text('\n'.join(lines)+'\n')
    root=[header]+[f'mod {module};' for _,module in modules]
    for fn,table,ret,_ in kinds:
        root.append(f'pub fn {fn}(class: &str, field: &str) -> Option<{ret}> {{')
        if fn=='sequence':root.append('    if class == "SpellLevelProgression" { return spell_progression(field); }')
        if not table:root.append('    let _ = field;')
        root.append('    match class {')
        for cls,module in modules:
            if any(k.startswith(cls+'.') for k in table):root.append(f'        "{cls}" => {module}::{fn}(field),')
        root.append('        _ => None,\n    }\n}')
    dispatch=[]
    dispatch.append('pub fn spell_progression(field: &str) -> Option<&\'static [i64]> {\n    let class = match field {')
    for field,cls in progression_routes.items(): dispatch.append(f'        "{field}" => "{cls}",')
    dispatch.append('        _ => return None,\n    };\n    super::sequence(class, field)\n}')
    (out/'progression_dispatch.rs').write_text(header+'\n'.join(dispatch)+'\n')
    root.extend(['mod progression_dispatch;', 'pub use progression_dispatch::spell_progression;'])
    (out/'spell_level_progression.rs').unlink(missing_ok=True)
    root.extend(['mod enum_values;', 'pub use enum_values::enum_value;', 'mod spell_index;', 'pub use spell_index::spell_levels;'])
    # Fixed records + pooled UTF-8 keys. Compile-time immutable source data,
    # not a gameplay save. Sorted keys support bounded allocation-free lookup.
    import zlib
    keys=sorted((k,int(v)) for k,v in symbols.items() if '.' in k and -(1<<63)<=v<(1<<63))
    pool=bytearray();records=bytearray()
    for key,v in keys:
        encoded=key.encode();records+=struct.pack('<IIq',len(pool),len(encoded),v);pool+=encoded
    payload=records+pool
    (out/'enum_values.bace').write_bytes(b'ACEENUM1'+struct.pack('<III',len(keys),len(pool),zlib.crc32(payload))+payload)
    (out/'mod.rs').write_text('\n'.join(root)+'\n')
    report={'pin':PIN,'sources':sources,'enum_sources':enum_sources,'chance_tables':len(chance),'float_tables':len(floats),'sequences':len(sequences),'references':len(references),'float_sequences':len(float_sequences),'rejected':rejected}
    (Path(__file__).with_name('extraction.json')).write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({k:v for k,v in report.items() if k not in ('sources','enum_sources')}))
if __name__=='__main__':main()
