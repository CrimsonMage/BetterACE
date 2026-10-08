#!/usr/bin/env python3
"""Compile original pinned ACE registry constructors/writers with narrow stubs."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile

PIN = "47edade3bd3f6044b676d4eb877c4965c7eda62b"
HERE = Path(__file__).resolve().parent
SOURCES = [
    "Source/ACE.Server/Network/Structure/Enchantment.cs",
    "Source/ACE.Server/Network/Structure/EnchantmentRegistry.cs",
    "Source/ACE.Entity/Models/PropertiesEnchantmentRegistry.cs",
    "Source/ACE.Entity/Enum/EnchantmentTypeFlags.cs",
    "Source/ACE.Entity/Enum/EnchantmentCategory.cs",
    "Source/ACE.Common/Extensions/FloatExtensions.cs",
]
parser = argparse.ArgumentParser()
parser.add_argument("--source", type=Path, required=True)
parser.add_argument("--dotnet", default="dotnet")
args = parser.parse_args()
hashes = []
with tempfile.TemporaryDirectory(prefix="bace-enchantments-") as directory:
    work = Path(directory)
    for index, relative in enumerate(SOURCES):
        data = (args.source / relative).read_bytes()
        hashes.append(f"# {relative} sha256={hashlib.sha256(data).hexdigest()}")
        (work / f"Source{index}.cs").write_bytes(data)
    (work / "Program.cs").write_bytes((HERE / "enchantments_harness.cs").read_bytes())
    (work / "Oracle.csproj").write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>')
    subprocess.run([args.dotnet,"build","--nologo","-o",str(work/"out"),str(work/"Oracle.csproj")],check=True)
    output = subprocess.check_output([args.dotnet,str(work/"out/Oracle.dll")],text=True)
    destination = HERE.parent / "tests/fixtures/enchantments.csv"
    destination.parent.mkdir(parents=True,exist_ok=True)
    destination.write_text(f"# Official ACEmulator/ACE {PIN}; AGPL-3.0-only; ACE contributors\n" + "\n".join(hashes) + "\n# Compiles full original constructors, grouping and serializers; synthetic definitions/entries.\n" + output)
