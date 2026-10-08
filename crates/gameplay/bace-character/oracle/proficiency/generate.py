#!/usr/bin/env python3
"""Compile pinned original Proficiency.cs, with queued GrantXP and skill-spend adapters."""
import pathlib, hashlib, subprocess, tempfile, sys
root=pathlib.Path(__file__).resolve().parents[5]
source=root/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Server/Entity/Proficiency.cs'
with tempfile.TemporaryDirectory() as tmp:
 p=pathlib.Path(tmp)
 (p/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 (p/'Proficiency.cs').write_bytes(source.read_bytes())
 (p/'Program.cs').write_bytes(pathlib.Path(__file__).with_name('harness.cs').read_bytes())
 run=subprocess.run([sys.argv[1],'run','--project',str(p),'--configuration','Release'],capture_output=True,text=True,check=False)
 if run.returncode: raise RuntimeError(run.stdout+run.stderr)
 lines=[x for x in run.stdout.splitlines() if x.startswith('CASE,')]
 target=pathlib.Path(__file__).parents[2]/'tests/fixtures/proficiency.csv'
 target.write_text('# ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b Entity/Proficiency.cs SHA256 '+hashlib.sha256(source.read_bytes()).hexdigest()+'\n# queued GrantXP and immediate pre-award skill spend adapters; no packets\n'+'\n'.join(lines)+'\n')
 print(len(lines),'original source vectors')
