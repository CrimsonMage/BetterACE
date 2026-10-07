#!/usr/bin/env python3
"""Compile verbatim pinned ACE expenditure/rank method bodies against fake assets.

The harness stubs only surrounding player/network/log/asset dependencies. No DAT,
player state, or credentials are redistributed. Output is synthetic test metadata.
"""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile
import urllib.request

PIN = "47edade3bd3f6044b676d4eb877c4965c7eda62b"
METHODS = {
    "Player_Attributes.cs": ["private bool SpendAttributeXp(", "public static int CalcAttributeRank("],
    "Player_Vitals.cs": ["private bool SpendVitalXp(", "public static int CalcVitalRank("],
    "Player_Skills.cs": ["private bool SpendSkillXp(", "public static int CalcSkillRank(", "public static List<uint> GetSkillXPTable(", "public bool TrainSkill(Skill skill, int creditsSpent,", "public bool SpecializeSkill(Skill skill, int creditsSpent,", "public bool HandleActionTrainSkill("],
    "Player_Xp.cs": ["public bool SpendXP("],
    "Source/ACE.Server/Factories/PlayerFactory.cs": ["private static CreateResult ValidateAttributeCredits("],
}


def method(source, signature):
    start = source.index(signature)
    body = source.index("{", start)
    depth = 0
    for index in range(body, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[start:index + 1]
    raise ValueError(f"Unterminated method: {signature}")


parser = argparse.ArgumentParser()
parser.add_argument("--source", type=Path)
parser.add_argument("--dotnet", default="dotnet")
args = parser.parse_args()
root = Path(__file__).resolve().parent
headers = [f"# official ACE {PIN}", "# synthetic table; kind,spent,available,amount,accepted,after_spent,after_rank,after_available"]
bodies = []
for filename, signatures in METHODS.items():
    path = filename if filename.startswith("Source/") else f"Source/ACE.Server/WorldObjects/{filename}"
    official = urllib.request.urlopen(
        f"https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{path}", timeout=30
    ).read()
    data = (args.source / path).read_bytes() if args.source else official
    if data != official:
        raise SystemExit(f"Not pinned official source: {path}")
    headers.append(f"# sha256 {hashlib.sha256(data).hexdigest()} {path}")
    bodies.extend(method(data.decode("utf-8-sig"), signature) for signature in signatures)

with tempfile.TemporaryDirectory(prefix="bace-character-oracle-") as temporary:
    build = Path(temporary)
    (build / "Oracle.csproj").write_text(
        '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType>'
        '<TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
    )
    harness = (root / "Program.cs").read_text()
    (build / "Program.cs").write_text(harness.replace("// OFFICIAL_METHODS", "\n".join(bodies)))
    subprocess.run([args.dotnet, "build", "--nologo", "-o", str(build / "out"), str(build / "Oracle.csproj")], check=True)
    result = subprocess.check_output([args.dotnet, str(build / "out/Oracle.dll")], text=True)
    progression = [line for line in result.splitlines() if not line.startswith(("creation,", "training,"))]
    creation = [line.removeprefix("creation,") for line in result.splitlines() if line.startswith("creation,")]
    (root.parent / "tests" / "fixtures" / "progression.csv").write_text("\n".join(headers + progression) + "\n")
    creation_headers = [headers[0], "# attr: six values,budget,ACE result; skill: advancement,credits,trained_cost,specialized_cost,success,remaining,xp,ranks,initial_level"] + headers[2:]
    (root.parent / "tests" / "fixtures" / "creation.csv").write_text("\n".join(creation_headers + creation) + "\n")
    training = [line.removeprefix("training,") for line in result.splitlines() if line.startswith("training,")]
    training_headers = [headers[0], "# kind,advancement,credits,price,quote,before_xp,before_init,success,after_advancement,after_xp,after_ranks,after_init,remaining"] + headers[2:]
    (root.parent / "tests" / "fixtures" / "training.csv").write_text("\n".join(training_headers + training) + "\n")
