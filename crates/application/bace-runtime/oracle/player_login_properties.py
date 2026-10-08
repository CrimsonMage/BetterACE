#!/usr/bin/env python3
"""Pinned ACE reflection is the authority for login property visibility."""
from pathlib import Path
import hashlib, subprocess, tempfile
ROOT=Path(__file__).resolve().parents[4]
SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Entity/Enum/Properties'
FAMILIES=['Int','Int64','Bool','Float','String','DataId','InstanceId']
def enum(text,name):
 start=text.index('public enum '+name);a=text.index('{',start);depth=1;b=a+1
 while depth:
  depth+=(text[b]=='{')-(text[b]=='}');b+=1
 return text[start:b]
files=[SRC/f'Property{x}.cs' for x in FAMILIES]+[SRC/f'{x}Attribute.cs' for x in ['SendOnLogin','AssessmentProperty','Ephemeral']]
with tempfile.TemporaryDirectory(prefix='bace-login-visibility-') as tmp:
 p=Path(tmp)
 for file in files:
  text=file.read_text()
  if file.stem.startswith('Property'):
   text='using System.ComponentModel;namespace ACE.Entity.Enum.Properties{'+enum(text,file.stem)+'}'
  (p/file.name).write_text(text)
 types=','.join('typeof(Property'+x+')' for x in FAMILIES)
 (p/'Program.cs').write_text('using System;using System.Linq;using System.Reflection;using ACE.Entity.Enum.Properties;class Program{static void Main(){foreach(var t in new[]{'+types+'}){var ids=t.GetFields(BindingFlags.Public|BindingFlags.Static).Where(f=>f.IsDefined(typeof(SendOnLoginAttribute),false)).Select(f=>Convert.ToUInt16(f.GetValue(null))).OrderBy(x=>x);Console.WriteLine(t.Name+"|"+string.Join(",",ids));}}}')
 (p/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
 subprocess.run(['/tmp/bace-crafting-dotnet/dotnet','build','--nologo','-o',str(p/'build')],cwd=p,check=True)
 output=subprocess.check_output(['/tmp/bace-crafting-dotnet/dotnet',str(p/'build/oracle.dll')],text=True)
 target=Path(__file__).resolve().parents[1]/'tests/fixtures/player_login_properties.csv'
 target.parent.mkdir(exist_ok=True)
 target.write_text(''.join('# '+f.name+' '+hashlib.sha256(f.read_bytes()).hexdigest()+'\n' for f in files)+output)
