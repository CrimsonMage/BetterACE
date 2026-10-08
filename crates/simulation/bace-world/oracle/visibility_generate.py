"""Compile pinned official ACE ObjectMaint/Position methods against small adapters."""
import argparse,hashlib,subprocess,tempfile,urllib.request
from pathlib import Path
PIN="47edade3bd3f6044b676d4eb877c4965c7eda62b"
p=argparse.ArgumentParser();p.add_argument("--source",type=Path,required=True);p.add_argument("--dotnet",default="dotnet");a=p.parse_args()
root=Path(__file__).resolve().parent
headers=[f"# Official ACEmulator/ACE {PIN}; AGPL-3.0-only; original methods with synthetic object/cell/lock adapters"]
def read(relative):
    raw=(a.source/relative).read_bytes();assert raw==urllib.request.urlopen(f"https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{relative}",timeout=30).read()
    headers.append(f"# {relative} sha256 {hashlib.sha256(raw).hexdigest()}");return raw.decode("utf-8-sig")
def method(source,signature):
    start=source.index(signature);brace=source.index("{",start);depth=0
    for end in range(brace,len(source)):
        depth+=(source[end]=="{")-(source[end]=="}")
        if depth==0:return source[start:end+1]
    raise ValueError(signature)
source=read("Source/ACE.Server/Physics/Common/ObjectMaint.cs")
position=read("Source/ACE.Server/Physics/Common/Position.cs")
read("Source/ACE.Server/Physics/PhysicsObj.cs")
harness=(root/"visibility_harness.cs").read_text()
signatures=["public List<PhysicsObj> GetVisibleObjects(ObjCell cell", "private List<PhysicsObj> GetVisibleObjects(EnvCell cell", "public bool AddVisibleObject(PhysicsObj obj)", "public bool AddObjectToBeDestroyed(PhysicsObj obj)", "public bool RemoveObjectToBeDestroyed(PhysicsObj obj)", "public List<PhysicsObj> DestroyObjects()"]
harness=harness.replace("// OBJECT_METHODS","\n".join(method(source,s) for s in signatures))
harness=harness.replace("// POSITION_METHOD",method(position,"public float Distance2DSquared(Position p)"))
with tempfile.TemporaryDirectory(prefix="betterace-visibility-") as temp:
    build=Path(temp);(build/"Program.cs").write_text(harness)
    (build/"Oracle.csproj").write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>disable</ImplicitUsings><Nullable>disable</Nullable></PropertyGroup></Project>')
    subprocess.run([a.dotnet,"build","--nologo","--verbosity","quiet","-o",str(build/"out"),str(build/"Oracle.csproj")],check=True)
    output=subprocess.check_output([a.dotnet,str(build/"out/Oracle.dll")],text=True)
world=root.parent/"tests/fixtures/visibility.csv"
replication=root.parents[2]/"network/bace-replication/tests/fixtures/visibility.csv"
for destination,kinds in [(world,("pvs,","distance,")),(replication,("clamp,","expiry,"))]:
    destination.parent.mkdir(parents=True,exist_ok=True)
    destination.write_text("\n".join(headers)+"\n"+"\n".join(line for line in output.splitlines() if line.startswith(kinds))+"\n")
