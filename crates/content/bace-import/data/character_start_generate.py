"""Convert pinned ACE starterGear.json to native TOML; JSON is tooling-only.

ACE contributors, AGPL-3.0-only. Authored gear/spell order is preserved.
"""
import argparse
import hashlib
import json
from pathlib import Path
import urllib.request

PIN = "47edade3bd3f6044b676d4eb877c4965c7eda62b"
RELATIVE = "Source/ACE.Server/starterGear.json"
parser = argparse.ArgumentParser()
parser.add_argument("--source", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
raw = (args.source / RELATIVE).read_bytes()
official = urllib.request.urlopen(f"https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{RELATIVE}", timeout=30).read()
if raw != official:
    raise SystemExit("starter gear differs from pinned official source")
# Explicit conversion correction: the pinned document has both weenieId and
# weenieID, while ConfigManager's original parser is case-sensitive. Normalize
# this known authoring discrepancy instead of silently losing two starter weapons.
def normalize(value):
    if isinstance(value, dict):
        result = {}
        for key, item in value.items():
            folded = key.lower()
            if folded in result:
                raise ValueError("ambiguous duplicate source property")
            result[folded] = normalize(item)
        return result
    if isinstance(value, list):
        return [normalize(item) for item in value]
    return value
data = normalize(json.loads(raw))
lines = [f"# ACE {PIN} {RELATIVE}", f"# SHA256 {hashlib.sha256(raw).hexdigest()}",
         "# ACE contributors; AGPL-3.0-only. Preserve authored sequence order.",
         "schema_version = 1", "human_template = 1", "default_start_spell = 3815", ""]
for skill in data["skills"]:
    groups = [(None, skill.get("gear", []))]
    groups += [(int(h["id"]), h.get("gear", [])) for h in skill.get("heritage", [])]
    for heritage, gear in groups:
        for item in gear:
            lines += ["[[gear]]", f"skill = {int(skill['id'])}"]
            if heritage is not None:
                lines += [f"heritage = {heritage}"]
            lines += [f"template = {int(item['weenieid'])}", f"count = {int(item.get('stacksize', 1))}", ""]
for skill in data["skills"]:
    for spell in skill.get("spells", []):
        specialized = spell.get("specializedonly", False)
        if isinstance(specialized, str):
            if specialized.lower() not in ("true", "false"):
                raise ValueError("invalid original specializedOnly")
            specialized = specialized.lower() == "true"
        lines += ["[[spells]]", f"skill = {int(skill['id'])}", f"spell = {int(spell['spellid'])}",
                  f"specialized_only = {str(specialized).lower()}", ""]
# PlayerFactory.Create's source switch. No guessed instantiation coordinates.
for name, spell in [("OlthoiLair", None), ("Shoushi", 3813), ("Yaraq", 3814), ("Sanamar", 3535), ("Holtburg", 3815)]:
    lines += ["[[starts]]", f'name = "{name}"']
    if spell is not None:
        lines += [f"free_ride_spell = {spell}"]
    lines += [""]
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text("\n".join(lines), encoding="utf-8")
