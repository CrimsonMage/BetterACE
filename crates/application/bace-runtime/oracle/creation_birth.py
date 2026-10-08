#!/usr/bin/env python3
"""Compile the pinned ACE Player constructor's exact DOB statement with an explicit UTC clock."""
import argparse, pathlib, tempfile, subprocess, urllib.request, hashlib
p=argparse.ArgumentParser();p.add_argument('--dotnet',default='dotnet');p.add_argument('--source',required=True);a=p.parse_args()
pin='47edade3bd3f6044b676d4eb877c4965c7eda62b';relative='Source/ACE.Server/WorldObjects/Player.cs'
raw=(pathlib.Path(a.source)/relative).read_bytes();official=urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{pin}/{relative}').read();assert raw==official
line=next(x.strip() for x in raw.decode().splitlines() if 'SetProperty(PropertyString.DateOfBirth,' in x)
with tempfile.TemporaryDirectory() as directory:
 d=pathlib.Path(directory);(d/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
 (d/'Program.cs').write_text('''using System;using System.Globalization;
static class PropertyString {public const int DateOfBirth=43;}
static class DateTime {public static System.DateTime UtcNow;}
class Program {static string value;static void SetProperty(int property,string text){if(property!=43)throw new Exception();value=text;}
static void Main(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;foreach(long clock in new long[]{0,951782400000,978307199999,1709164800000,1791417600000,253402300799999}) {DateTime.UtcNow=DateTimeOffset.FromUnixTimeMilliseconds(clock).UtcDateTime;
'''+line+'\nConsole.WriteLine($"{clock},{value}");}}}')
 subprocess.run([a.dotnet,'build',str(d/'oracle.csproj'),'-o',str(d/'out'),'--nologo'],check=True,stdout=subprocess.DEVNULL)
 result=subprocess.check_output([a.dotnet,str(d/'out/oracle.dll')],text=True)
 target=pathlib.Path(__file__).parent.parent/'tests/fixtures/creation_birth.csv';target.write_text(f'# ACE {pin} {relative}:101 SHA256 {hashlib.sha256(raw).hexdigest()}\n'+result)
