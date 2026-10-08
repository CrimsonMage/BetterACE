"""Compile unchanged local client/GDLE timing methods; never embed client source."""
import argparse
import hashlib
import subprocess
import tempfile
from pathlib import Path

PIN = "353cbab52ef7da2b7063bc3e3f008461d8531693"
CLIENT_SHA = "ffb745b53eb3bf764d96ba962e9003b6b4f55e9e57bc6356c2d613d15c43f734"
p = argparse.ArgumentParser()
p.add_argument("--source", type=Path, required=True)
p.add_argument("--client", type=Path, required=True)
a = p.parse_args()

def method(source, signature):
    begin = source.index(signature)
    brace = source.index("{", begin)
    depth = 0
    for end in range(brace, len(source)):
        depth += (source[end] == "{") - (source[end] == "}")
        if depth == 0:
            return source[begin:end + 1]
    raise ValueError(signature)

headers = [f"# GDLE {PIN}; compiled original methods, synthetic adapters; AGPL-3.0-only",
           "# Client build provenance unconfirmed; INDEX summaries/domain09/CombatSystem/ClientCombatSystem route"]
client = a.client.read_bytes()
assert hashlib.sha256(client).hexdigest() == CLIENT_SHA
headers.append(f"# ClientCombatSystem.cpp sha256 {CLIENT_SHA}; GetPowerBarLevel 0056ADE0")
gdle_path = "Source/AttackManager.cpp"
gdle = (a.source / gdle_path).read_bytes()
assert gdle == subprocess.check_output(["git", "-C", str(a.source), "show", f"{PIN}:{gdle_path}"])
headers.append(f"# {gdle_path} sha256 {hashlib.sha256(gdle).hexdigest()}")
harness = Path(__file__).with_name("physical_timing_harness.cpp").read_text()
harness = harness.replace("// CLIENT_METHOD", method(client.decode(), "long double __thiscall ClientCombatSystem::GetPowerBarLevel("))
harness = harness.replace("// GDLE_METHOD", method(gdle.decode(), "void AttackManager::OnAttackDone("))
with tempfile.TemporaryDirectory(prefix="betterace-timing-") as temporary:
    root = Path(temporary)
    (root / "oracle.cpp").write_text(harness)
    subprocess.run(["c++", "-std=c++17", "-O0", "-ffp-contract=off", str(root / "oracle.cpp"), "-o", str(root / "oracle")], check=True)
    output = subprocess.check_output([str(root / "oracle")], text=True)
Path(__file__).parents[1].joinpath("tests/fixtures/physical_timing.csv").write_text("\n".join(headers) + "\n" + output)
