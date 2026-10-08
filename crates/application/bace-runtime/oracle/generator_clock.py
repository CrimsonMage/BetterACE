#!/usr/bin/env python3
"""Compile unmodified pinned ACE DerethDateTime for generator day boundaries."""
from pathlib import Path
import tempfile,subprocess,hashlib
ROOT=Path(__file__).resolve().parents[4]
source=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Common/DerethDateTime.cs'
with tempfile.TemporaryDirectory(prefix='bace-dereth-clock-') as tmp:
 p=Path(tmp);(p/'DerethDateTime.cs').write_bytes(source.read_bytes())
 (p/'Program.cs').write_text('''using System;using System.Collections.Generic;using System.Globalization;using ACE.Common;
class Program{static void Main(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;var values=new List<double>{0,DerethDateTime.MaxValue,DerethDateTime.MaxValue-0.001,300000000};for(int quarter=0;quarter<=128;quarter++)foreach(var delta in new[]{-0.000001,0.0,0.000001}){double value=quarter*119.0625+59.53125+delta;if(value>=0)values.Add(value);}foreach(var value in values){var clock=new DerethDateTime(value);Console.WriteLine($"{value:R},{clock.Hour},{clock.IsDay}");}}}''')
 (p/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
 subprocess.run(['/tmp/bace-crafting-dotnet/dotnet','build','--nologo','-o',str(p/'build')],cwd=p,check=True)
 output=subprocess.check_output(['/tmp/bace-crafting-dotnet/dotnet',str(p/'build/oracle.dll')],text=True)
 (Path(__file__).resolve().parents[1]/'tests/fixtures/generator_clock.csv').write_text('# ACE.Common/DerethDateTime.cs '+hashlib.sha256(source.read_bytes()).hexdigest()+'\n'+output)
