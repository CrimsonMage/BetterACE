"""Compile original pinned ACE MotionInterp methods; no DAT or captures."""
import argparse, hashlib, subprocess, tempfile, urllib.request
from pathlib import Path
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args()
root=Path(__file__).resolve().parent
headers=[f'# Official ACEmulator/ACE {PIN}; AGPL-3.0-only; original adjust_motion/apply_run_to_command; synthetic run-rate adapter']
def read(name):
 data=(a.source/name).read_bytes();assert data==urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{name}',timeout=30).read();headers.append(f'# {name} sha256 {hashlib.sha256(data).hexdigest()}');return data.decode('utf-8-sig')
source=read('Source/ACE.Server/Physics/Animation/MotionInterp.cs')
enums={name:read(f'Source/ACE.Entity/Enum/{name}.cs') for name in ['MotionCommand','HoldKey']}
def method(name):
 start=source.index('public void '+name+'(');brace=source.index('{',start);depth=1;end=brace+1
 while depth:depth+=(source[end]=='{')-(source[end]=='}');end+=1
 return source[start:end]
constants='\n'.join(line for line in source.splitlines() if 'public const float ' in line)
harness='''using System; using System.Globalization; using ACE.Entity.Enum;
class Weenie {public bool IsCreature=true; public float Rate; public bool InqRunRate(ref float r){r=Rate;return true;}}
class Raw {public HoldKey CurrentHoldKey;}
class Interp {public Weenie WeenieObj=new();public Raw RawState=new();public float MyRunRate=1; METHODS }
class Program {static void Main(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
uint[][] commands={new uint[]{0x45000005,0x45000006},new uint[]{0x6500000f,0x65000010},new uint[]{0x6500000d,0x6500000e}};
for(int axis=0;axis<3;axis++)foreach(uint command in commands[axis])foreach(uint current in new uint[]{1,2})foreach(uint hold in new uint[]{0,1,2})foreach(float speed in new float[]{-1f,-.25f,.25f,1f})foreach(float rate in new float[]{.8f,1f,2.5f}){
var interp=new Interp();interp.RawState.CurrentHoldKey=(HoldKey)current;interp.WeenieObj.Rate=rate;var output=command;var value=speed;interp.adjust_motion(ref output,ref value,(HoldKey)hold);Console.WriteLine($"{axis},{current},{hold},{command},{speed:R},{rate:R},{output},{value:R}");}
}}'''.replace('METHODS',constants+method('adjust_motion')+method('apply_run_to_command'))
with tempfile.TemporaryDirectory(prefix='betterace-raw-motion-') as tmp:
 d=Path(tmp);(d/'Program.cs').write_text(harness)
 for name,text in enums.items():(d/(name+'.cs')).write_text(text)
 (d/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
 subprocess.run([a.dotnet,'build','--nologo','--verbosity','quiet','-o',str(d/'out'),str(d/'Oracle.csproj')],check=True)
 output=subprocess.check_output([a.dotnet,str(d/'out/Oracle.dll')],text=True)
(root.parent/'tests/fixtures/physical_locomotion.csv').write_text('\n'.join(headers)+'\n'+output)
