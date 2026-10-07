"""Generate editor enum labels from the official ACE pin, not Weenie-Fab.

Upstream names: ACEmulator/ACE, AGPL-3.0-only. Labels are display assistance;
unknown numeric IDs are always retained and accepted by the editor.
"""
import hashlib
import pathlib
import re
import sys

PIN = "47edade3bd3f6044b676d4eb877c4965c7eda62b"
root = pathlib.Path(__file__).resolve().parents[1]
source = pathlib.Path(sys.argv[1]) / "Source/ACE.Entity/Enum"
families = {
    "ints": "Properties/PropertyInt.cs", "int64s": "Properties/PropertyInt64.cs",
    "bools": "Properties/PropertyBool.cs", "floats": "Properties/PropertyFloat.cs",
    "strings": "Properties/PropertyString.cs", "data_ids": "Properties/PropertyDataId.cs",
    "instance_ids": "Properties/PropertyInstanceId.cs", "positions": "Properties/PositionType.cs",
    "attributes": "Properties/PropertyAttribute.cs", "secondary_attributes": "Properties/PropertyAttribute2nd.cs",
    "skills": "Skill.cs", "body_parts": "CombatBodyPart.cs", "weenie_type": "WeenieType.cs",
    "category": "EmoteCategory.cs", "type": "EmoteType.cs", "spell_book": "SpellId.cs",
}
rows = ["# Official ACEmulator/ACE " + PIN + "; AGPL-3.0-only"]
manifest = ['version = 1', f'upstream_pin = "{PIN}"', 'license = "AGPL-3.0-only"']
for family, path in families.items():
    data = (source / path).read_bytes()
    manifest += ['\n[[sources]]', f'path = "Source/ACE.Entity/Enum/{path}"',
                 f'sha256 = "{hashlib.sha256(data).hexdigest()}"']
    text = re.sub(r'/\*.*?\*/', '', data.decode('utf-8-sig'), flags=re.S)
    body = re.search(r'public enum \w+[^\{]*\{([^}]+)\}', text, re.S).group(1)
    body = re.sub(r'//[^\n]*', '', body)
    body = re.sub(r'\[[^\]]*\]', '', body)
    current = -1
    for entry in body.split(','):
        entry = entry.strip()
        if not entry:
            continue
        number_pattern = r'-?(?:0x[0-9a-fA-F]+|[0-9]+)'
        match = re.fullmatch(r'(\w+)(?:\s*=\s*(' + number_pattern + r'(?:\s*\|\s*' + number_pattern + r')*))?', entry)
        if not match:
            raise ValueError(f'Unsupported enum expression in {path}: {entry}')
        name, number = match.groups()
        if number:
            parts = [int(part.strip(), 16 if '0x' in part else 10) for part in number.split('|')]
            current = parts[0]
            for part in parts[1:]:
                current |= part
        else:
            current += 1
        rows.append(f'{family}\t{current}\t{name}')
(root / 'data/property-labels.tsv').write_text('\n'.join(rows)+'\n')
(root / 'data/provenance.toml').write_text('\n'.join(manifest)+'\n')
