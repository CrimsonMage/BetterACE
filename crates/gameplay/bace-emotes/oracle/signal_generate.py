"""Compile pinned CylinderDistance with explicit same-landblock Position stubs."""
import argparse,hashlib,subprocess,tempfile
from pathlib import Path
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b';REL='Source/ACE.Server/Physics/Common/Position.cs'
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args()
data=subprocess.check_output(['git','-C',str(a.source),'show',f'{PIN}:{REL}']);source=data.decode('utf-8-sig');start=source.index('public static double CylinderDistance(');op=source.index('{',start);depth=0
for end in range(op,len(source)):
 depth+=(source[end]=='{')-(source[end]=='}')
 if depth==0:method=source[start:end+1];break
h='''using System; using System.Numerics; using System.Globalization;
class Frame { public Vector3 Origin; }
class Position { public Frame Frame=new Frame(); public Vector3 GetOffset(Position p)=>p.Frame.Origin-Frame.Origin;
METHOD
static void Main(){
 foreach(var r in new float[][]{new float[]{0,0,0,0,0,0,.5f,2,.5f,2},new float[]{0,0,0,3,4,0,.5f,2,.5f,2},new float[]{0,0,0,0,0,5,.5f,2,.5f,2},new float[]{0,0,5,0,0,0,.5f,2,.5f,2},new float[]{0,0,0,1,0,1,.5f,2,.5f,2},new float[]{0,0,0,1,0,0,.5f,2,.5f,2},new float[]{0,0,0,.9999999f,0,0,.5f,2,.5f,2},new float[]{0,0,0,1.0000001f,0,0,.5f,2,.5f,2}}){
 var a=new Position();a.Frame.Origin=new Vector3(r[0],r[1],r[2]);var b=new Position();b.Frame.Origin=new Vector3(r[3],r[4],r[5]);
 Console.WriteLine(string.Join(",",Array.ConvertAll(r,x=>x.ToString("R",CultureInfo.InvariantCulture)))+","+CylinderDistance(r[6],r[7],a,r[8],r[9],b).ToString("R",CultureInfo.InvariantCulture));
 }
}}
'''.replace('METHOD',method)
with tempfile.TemporaryDirectory(prefix='bace-npc-signal-') as td:
 b=Path(td);(b/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><RollForward>Major</RollForward></PropertyGroup></Project>');(b/'Program.cs').write_text(h)
 subprocess.run([a.dotnet,'build','--nologo','-o',str(b/'out'),str(b/'Oracle.csproj')],check=True)
 output=subprocess.check_output([a.dotnet,str(b/'out/Oracle.dll')],text=True)
(Path(__file__).parent.parent/'tests/fixtures/signal_distance.csv').write_text(f'# official ACE {PIN}; unchanged CylinderDistance, explicit same-landblock frame stubs\n# sha256 {hashlib.sha256(data).hexdigest()} {REL}\n'+output)
