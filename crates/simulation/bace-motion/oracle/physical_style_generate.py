"""Independent pinned GDLE CM_Style branch oracle, lookup adapters only."""
import argparse, hashlib, subprocess, tempfile
from pathlib import Path
PIN="353cbab52ef7da2b7063bc3e3f008461d8531693"
p=argparse.ArgumentParser();p.add_argument("--source",type=Path,required=True);a=p.parse_args()
relative="Source/PhatSDK/MotionTable.cpp"
raw=(a.source/relative).read_bytes()
assert raw==subprocess.check_output(["git","-C",str(a.source),"show",f"{PIN}:{relative}"])
source=raw.decode()
def method(signature):
    start=source.index(signature);brace=source.index("{",start);depth=0
    for end in range(brace,len(source)):
        depth+=(source[end]=="{")-(source[end]=="}")
        if depth==0:return source[start:end+1]
    raise ValueError(signature)
harness=Path(__file__).with_name("physical_style_harness.cpp").read_text()
harness=harness.replace("// LINK",method("MotionData *CMotionTable::get_link("))
harness=harness.replace("// STYLE",method("\tif (motionid & CM_Style)"))
with tempfile.TemporaryDirectory(prefix="betterace-style-") as temp:
    root=Path(temp);(root/"main.cpp").write_text(harness)
    subprocess.run(["c++","-std=c++17","-O0","-ffp-contract=off",str(root/"main.cpp"),"-o",str(root/"oracle")],check=True)
    output=subprocess.check_output([str(root/"oracle")],text=True)
Path(__file__).parents[1].joinpath("tests/fixtures/physical_style.csv").write_text(f"# GDLE {PIN}; {relative} sha256 {hashlib.sha256(raw).hexdigest()}; original get_link and CM_Style branch; AGPL-3.0-only\n"+output)
