#!/usr/bin/env python3
"""Generate WCID leaf descriptors from hash-verified pinned ACE declarations.

Values remain in the existing independently checked source tables. This emits
only typed field names; Boolean selectors and table-reference lists are excluded.
AGPL-3.0-only, ACE contributors and BetterACE contributors.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
from generate import PIN, ROOT, clean

parser = argparse.ArgumentParser()
parser.add_argument('--ace', type=Path, default=ROOT / '.reference' / ('ACE-' + PIN))
args = parser.parse_args()
provenance = json.loads(Path(__file__).with_name('extraction.json').read_text())
assert provenance['pin'] == PIN
pattern = re.compile(r'\bstatic\s+(?:readonly\s+)?(ChanceTable<WeenieClassName>|List<WeenieClassName>|ChanceTable<GemResult>)\s+(\w+)\s*=')
rows = []
for relative, expected in sorted(provenance['sources'].items()):
    path = args.ace / relative
    content = path.read_bytes()
    if hashlib.sha256(content).hexdigest() != expected:
        raise ValueError('source hash mismatch: ' + relative)
    text = clean(content.decode('utf-8-sig'))
    name = re.search(r'\bclass\s+(\w+)', text)
    if not name:
        continue
    for kind, field in pattern.findall(text):
        method = {'ChanceTable<WeenieClassName>': 'Chance', 'List<WeenieClassName>': 'Sequence', 'ChanceTable<GemResult>': 'Gem'}[kind]
        rows.append((name[1], field, method))
if not rows or len(rows) > 1024 or len(rows) != len(set(rows)):
    raise ValueError('source closure bounds/duplicates')
target = Path(__file__).with_name('template-leaves.toml')
target.write_text('# Direct WCID fields from official ACE ' + PIN + '.\n'
                  '# Copyright ACE contributors. AGPL-3.0-only.\n'
                  'fields = [' + ', '.join(json.dumps(name + '.' + field)
                         for name, field, _ in sorted(rows)) + ']\n')
print(f'{len(rows)} pinned template fields -> {target}')
