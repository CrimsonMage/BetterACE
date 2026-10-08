#!/usr/bin/env python3
"""Compile unchanged ACE Stackable defaults and SetStackSize, on fresh templates."""
import argparse, hashlib, json, pathlib, subprocess, tempfile
p=argparse.ArgumentParser();p.add_argument('--source',type=pathlib.Path,required=True);p.add_argument('--dotnet',required=True);a=p.parse_args()
base=a.source/'Source/ACE.Server/WorldObjects'
def method(path,signature):
 s=path.read_text(encoding='utf-8-sig');start=s.index(signature);opening=s.index('{',start);depth=1;end=opening+1
 while depth:
  depth += (s[end]=='{')-(s[end]=='}');end+=1
 return s[start:end]
defaults=method(base/'Stackable.cs','private void SetEphemeralValues()')
set_size=method(base/'WorldObject_Properties.cs','public void SetStackSize(int? value)')
program='''using System;
class Stackable {
 public int? StackSize,MaxStackSize,Value,EncumbranceVal,StackUnitEncumbrance,StackUnitValue;
'''+defaults+'\n'+set_size+'''
 public void Init(){SetEphemeralValues();}
}
class Program {static void Main(){
 int?[][] cases = {new int?[]{10,100,30,20,2,3},new int?[]{10,100,37,29,null,null},new int?[]{1,100,17,13,null,null},new int?[]{null,100,null,null,null,null},new int?[]{5,100,999,999,7,11}};
 foreach(var c in cases){var s=new Stackable{StackSize=c[0],MaxStackSize=c[1],Value=c[2],EncumbranceVal=c[3],StackUnitEncumbrance=c[4],StackUnitValue=c[5]};s.Init();s.SetStackSize(3);Console.WriteLine(string.Join(",",Array.ConvertAll(c,v=>v?.ToString()??"-"))+$",{s.StackUnitEncumbrance},{s.StackUnitValue},{s.StackSize},{s.EncumbranceVal},{s.Value}");}
}}
'''
root=pathlib.Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='bace-split-oracle-') as temp:
 t=pathlib.Path(temp);(t/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>');(t/'Program.cs').write_text(program)
 subprocess.run([a.dotnet,'build','--nologo','-o',str(t/'out'),str(t/'Oracle.csproj')],check=True)
 output=subprocess.check_output([a.dotnet,str(t/'out/Oracle.dll')],text=True)
 (root/'tests/fixtures/split_factory.csv').write_text('# template size,max,value,burden,unitburden,unitvalue,result unitburden,unitvalue,size,burden,value\n'+output)
 (root/'tests/fixtures/split_factory.provenance.json').write_text(json.dumps({'repository':'https://github.com/ACEmulator/ACE','commit':'47edade3bd3f6044b676d4eb877c4965c7eda62b','sources':{str(f.relative_to(a.source)):hashlib.sha256(f.read_bytes()).hexdigest() for f in [base/'Stackable.cs',base/'WorldObject_Properties.cs']},'extracted_sha256':hashlib.sha256((defaults+set_size).encode()).hexdigest(),'scope':'unchanged Stackable.SetEphemeralValues and WorldObject.SetStackSize; fresh factory field harness, not complete handler parity'},indent=2)+'\n')
