#!/usr/bin/env python3
"""Compile verbatim official quest eligibility/predicate bodies with fake I/O."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile
import urllib.request

PIN = "47edade3bd3f6044b676d4eb877c4965c7eda62b"
SOURCE = "Source/ACE.Server/Managers/QuestManager.cs"
SIGNATURES = [
    "public static bool CanScaleQuestMinDelta(",
    "public TimeSpan GetNextSolveTime(",
    "public static string GetQuestName(",
    "public bool HasQuestSolves(",
    "public bool HasQuestBits(",
    "public bool HasNoQuestBits(",
]


def extract(source, signature):
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
    raise ValueError(signature)


parser = argparse.ArgumentParser()
parser.add_argument("--source", type=Path)
parser.add_argument("--dotnet", default="dotnet")
args = parser.parse_args()
root = Path(__file__).resolve().parent
official = urllib.request.urlopen(f"https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{SOURCE}", timeout=30).read()
data = (args.source / SOURCE).read_bytes() if args.source else official
if data != official:
    raise SystemExit("Source is not pinned official ACE")
methods = "\n".join(extract(data.decode("utf-8-sig"), signature) for signature in SIGNATURES)
with tempfile.TemporaryDirectory(prefix="bace-quest-oracle-") as temporary:
    build = Path(temporary)
    (build / "Oracle.csproj").write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>')
    (build / "Program.cs").write_text((root / "Program.cs").read_text().replace("// OFFICIAL_METHODS", methods))
    subprocess.run([args.dotnet, "build", "--nologo", "-o", str(build / "out"), str(build / "Oracle.csproj")], check=True)
    result = subprocess.check_output([args.dotnet, str(build / "out/Oracle.dll")], text=True)
    (root.parent / "tests" / "fixtures" / "eligibility.csv").write_text(
        f"# official ACE {PIN}\n# sha256 {hashlib.sha256(data).hexdigest()} {SOURCE}\n" + result
    )
