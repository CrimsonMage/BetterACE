#!/usr/bin/env python3
"""Export independently verified ACE rows and imported scripts as native TOML.

Table rows are verified original-source fixtures; enums and spell routes are
resolved directly from pinned ACE source with generate.py's enum parser.
The TOML is tooling input; runtime consumes only its versioned .bace record.
"""
import csv
import json
import tomllib
from pathlib import Path
import re

from generate import clean, extract_enums

ROOT = Path(__file__).resolve().parents[5]
HERE = Path(__file__).resolve().parent
PIN = '47edade3bd3f6044b676d4eb877c4965c7eda62b'
OUT = ROOT / 'crates/gameplay/bace-loot/data/ace-treasure-tables.toml'


def main():
    fields = {}
    fixture = ROOT / 'crates/gameplay/bace-loot/tests/fixtures/ace-tables.csv'
    with fixture.open() as stream:
        for row in csv.reader(stream, delimiter='|'):
            if not row or row[0].startswith('#'):
                continue
            kind, full, *values = row
            fields.setdefault(full, {}).setdefault(kind, []).append(values)
    lines = ['# Extracted from official ACEmulator/ACE ' + PIN,
             '# ACE contributor attribution and hashes: oracle/tables/extraction.json',
             'schema_version = 1', 'id = 1', 'source_pin = ' + json.dumps(PIN), '']
    leaves = HERE / 'template-leaves.toml'
    leaf_fields = tomllib.loads(leaves.read_text())['fields']
    lines.insert(5, 'template_leaf_tables = [' + ', '.join(json.dumps(name) for name in leaf_fields) + ']')
    keys = {'chance':'chance', 'float':'float_sequence_bits', 'gem':'gem',
            'sequence':'sequence', 'reference':'references', 'typed':'typed_references',
            'float_sequence':'float_sequence_bits', 'descriptor':'descriptors'}
    for full, kinds in sorted(fields.items()):
        cls, field = full.split('.', 1)
        assert len(kinds) == 1, full
        kind, rows = next(iter(kinds.items()))
        lines += ['[[tables]]', 'class = ' + json.dumps(cls), 'field = ' + json.dumps(field)]
        if kind == 'chance' and cls == 'MissileMagicDefense':
            value = [f'{{ value_bits = {r[0]}, probability_bits = {r[1]} }}' for r in rows]
            kind = 'float_chance'
        elif kind == 'chance':
            value = [f'{{ value = {r[0]}, probability_bits = {r[1]} }}' for r in rows]
        elif kind == 'float':
            value = [r[0] for r in rows]
        elif kind == 'gem':
            value = [f'{{ wcid = {r[0]}, material = {r[1]}, probability_bits = {r[2]} }}' for r in rows]
        elif kind == 'sequence':
            value = [r[0] for r in rows]
        elif kind == 'reference':
            value = [json.dumps(r[0]) for r in rows]
        elif kind == 'typed':
            value = [f'{{ name = {json.dumps(r[0])}, value = {r[1]} }}' for r in rows]
        elif kind == 'descriptor':
            value = [f'{{ name = {json.dumps(r[0])}, text = {json.dumps(r[1])} }}' for r in rows]
        else:
            raise ValueError(kind)
        lines += [('float_chance' if kind == 'float_chance' else keys[kind]) + ' = [' + ', '.join(value) + ']', '']
    ace_root = ROOT / '.reference' / ('ACE-' + PIN)
    symbols, _, _ = extract_enums(ace_root / 'Source', ace_root)
    # The dictionary follows pinned source declaration order within each enum.
    # Preserve it for consumers that enumerate flags or choices.
    enums = [(name, int(value)) for name, value in symbols.items()
             if '.' in name and -(1 << 63) <= value < (1 << 63)]
    for name, value in enums:
        # Guard the pinned extractor correction: C# attributes containing
        # commas must be stripped before member splitting in generate.py.
        assert '[SuppressMessage' not in name, (name, value)
        lines += ['[[enums]]', 'name = ' + json.dumps(name), f'value = {value}', '']
    progression = {name.split('.', 1)[1]: name for name in fields if name.startswith('SpellLevelProgression.')}
    progression_source = clean((ace_root / 'Source/ACE.Server/Factories/Tables/SpellLevelProgression.cs').read_text())
    by_spell = {}
    for field in re.findall(r'AddSpells\((\w+)\);', progression_source):
        rows = fields[progression[field]]['sequence']
        for (number,) in rows:
            spell = int(number)
            if spell == 0:
                continue
            assert spell not in by_spell, ('duplicate spell route', spell)
            by_spell[spell] = progression[field]
    for spell, name in sorted(by_spell.items()):
        lines += ['[[spell_routes]]', 'name = ' + json.dumps(name), f'value = {spell}', '']
    script_root = ROOT / 'crates/gameplay/bace-loot/oracle/mutations/scripts'
    for path in sorted(script_root.rglob('*.txt')):
        name = str(path.relative_to(script_root)).replace('/', '.')
        lines += ['[[scripts]]', 'name = ' + json.dumps(name),
                  'text = ' + json.dumps(path.read_text()), '']
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text('\n'.join(lines))
    print(f'{len(fields)} tables, {len(enums)} enums, {len(list(script_root.rglob("*.txt")))} scripts: {OUT}')


if __name__ == '__main__':
    main()
