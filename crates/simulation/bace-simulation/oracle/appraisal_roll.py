#!/usr/bin/env python3
"""Source-derived Player.Examine vectors; no C# runtime is used.

The script verifies pinned ACE method hashes and operation order before
evaluating the source's small, bounded fixture inputs. Run from any directory.
"""

import hashlib
import math
import os
import pathlib
import struct
import subprocess
import sys

PIN = "47edade3bd3f6044b676d4eb877c4965c7eda62b"
PLAYER_HASH = "01b0ba47863708e135e40141f0672a9841d7d7ad2a5091a0f7007fcb729047cc"
SKILL_HASH = "cc457426154a151ad0174fc1cb2ceb7af158afe38f3ad8ef05da65d0c556236d"
ROOT = pathlib.Path(__file__).resolve().parents[4]
ACE = pathlib.Path(os.environ.get("BACE_ACE_SOURCE", ROOT.parent / "ACE"))
FIXTURE = pathlib.Path(__file__).resolve().parents[1] / "tests/fixtures/appraisal_roll.tsv"


def pinned(path: str, digest: str) -> str:
    source = subprocess.check_output(["git", "show", f"{PIN}:{path}"], cwd=ACE)
    assert hashlib.sha256(source).hexdigest() == digest, path
    return source.decode("utf-8-sig")


player = pinned("Source/ACE.Server/WorldObjects/Player.cs", PLAYER_HASH)
skill_check = pinned("Source/ACE.Server/WorldObjects/SkillCheck.cs", SKILL_HASH)
method = player.split("public void Examine(WorldObject obj)", 1)[1].split(
    "public void OnAppraisal", 1
)[0]
ordered = [
    "var success = true;",
    "var creature = obj as Creature;",
    "var currentSkill = (int)GetCreatureSkill(skill).Current;",
    "int difficulty = (int)creature.GetCreatureSkill(Skill.Deception).Current;",
    'PropertyManager.GetBool("assess_creature_mod")',
    "var chance = SkillCheck.GetSkillChance(currentSkill, difficulty);",
    "if (difficulty == 0 || player == this || player != null && !player.GetCharacterOption",
    "if ((this is Admin || this is Sentinel) && CloakStatus == CloakStatus.On)",
    "success = chance > ThreadSafeRandom.Next(0.0f, 1.0f);",
    "if (obj.ResistItemAppraisal >= 999)",
    "if (creature is Pet || creature is CombatPet)",
]
position = 0
for token in ordered:
    found = method.find(token, position)
    assert found >= 0, token
    position = found + len(token)
assert "1.0 - (1.0 / (1.0 + Math.Exp(factor * (skill - difficulty))))" in skill_check
assert "float factor = 0.03f" in skill_check


def single(value: float) -> float:
    return struct.unpack("<f", struct.pack("<f", value))[0]


# name, kind, skill, difficulty, resist, mod, untrained, focus, self,
# cloak, self_target, attempt_deceive, draw; all integers are small and
# nonnegative, so the source's int subtraction and uint addition cannot wrap.
CASES = [
    ("plain_item", "object", 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, "-"),
    ("resistant_item", "object", 0, 0, 999, 0, 0, 0, 0, 0, 0, 0, "-"),
    ("monster_over", "monster", 0, 100, 0, 0, 1, 100, 100, 0, 0, 0, "0.05"),
    ("monster_under", "monster", 0, 100, 0, 0, 1, 100, 100, 0, 0, 0, "0.04"),
    ("modifier_off", "monster", 0, 100, 0, 0, 1, 100, 100, 0, 0, 0, "0.49"),
    ("modifier_on", "monster", 0, 100, 0, 1, 1, 100, 100, 0, 0, 0, "0.49"),
    ("modifier_equal_fails", "monster", 0, 100, 0, 1, 1, 100, 100, 0, 0, 0, "0.5"),
    ("modifier_below_wins", "monster", 0, 100, 0, 1, 1, 100, 100, 0, 0, 0, "0.49999997"),
    ("trained_ignores_mod", "monster", 0, 100, 0, 1, 0, 100, 100, 0, 0, 0, "0.49"),
    ("zero_deception", "monster", 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, "0.99"),
    ("player_no_deceive", "player", 0, 100, 0, 1, 1, 100, 100, 0, 0, 0, "0.99"),
    ("player_deceives", "player", 0, 100, 0, 1, 1, 100, 100, 0, 0, 1, "0.99"),
    ("self_deceives", "player", 0, 100, 0, 0, 0, 0, 0, 0, 1, 1, "0.99"),
    ("cloaked_staff", "monster", 0, 100, 0, 0, 0, 0, 0, 1, 0, 0, "0.99"),
    ("resist_after_cloak", "monster", 0, 100, 999, 0, 0, 0, 0, 1, 0, 0, "0.99"),
    ("pet_after_resist", "pet", 0, 100, 999, 0, 0, 0, 0, 0, 0, 0, "0.99"),
]


def result(row: tuple) -> tuple[int, int]:
    _, kind, skill, difficulty, resist, modified, untrained, focus, self_attr, cloak, is_self, deceive, raw_draw = row
    success = True
    consumed = kind != "object"
    if consumed:
        if kind != "player" and modified and untrained:
            skill = (focus + self_attr) // 2
        exponent = single(single(0.03) * single(skill - difficulty))
        chance = max(0.0, min(1.0, 1.0 - 1.0 / (1.0 + math.exp(exponent))))
        if difficulty == 0 or kind == "player" and (is_self or not deceive) or cloak:
            chance = 1.0
        success = chance > single(float(raw_draw))
    if resist >= 999:
        success = False
    if kind == "pet":
        success = True
    return int(success), int(consumed)


header = (
    f"# ACE {PIN} Player.cs sha256 {PLAYER_HASH}\n"
    f"# SkillCheck.cs sha256 {SKILL_HASH}; source-derived, not executed C# output\n"
    "# name\tkind\tskill\tdifficulty\tresist\tmod\tuntrained\tfocus\tself\tcloak\tself_target\tdeceive\tdraw\tsuccess\tconsumed\n"
)
contents = header + "".join(
    "\t".join(map(str, (*row, *result(row)))) + "\n" for row in CASES
)
if sys.argv[1:] == ["--check"]:
    assert FIXTURE.read_text() == contents, "appraisal_roll.tsv is stale"
else:
    sys.stdout.write(contents)
