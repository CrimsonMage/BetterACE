#!/usr/bin/env python3
"""Pinned ACE login statements and original UShortSequence implementation."""
from pathlib import Path
import hashlib
import subprocess
import tempfile
ROOT=Path(__file__).resolve().parents[4]
SOURCE=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
player=SOURCE/'ACE.Server/WorldObjects/Player_Networking.cs'
sequence=SOURCE/'ACE.Server/Network/Sequence/UShortSequence.cs'
interface=SOURCE/'ACE.Server/Network/Sequence/ISequence.cs'
lines=player.read_text().splitlines()
statements='\n'.join(next(line for line in lines if phrase in line) for phrase in [
    'Character.TotalLogins++;', 'Sequences.SetSequence(SequenceType.ObjectInstance, new UShortSequence((ushort)Character.TotalLogins));'])
with tempfile.TemporaryDirectory(prefix='bace-login-instance-') as directory:
    p=Path(directory)
    for source in [sequence,interface]: (p/source.name).write_text(source.read_text())
    (p/'Program.cs').write_text('''
using System; using ACE.Server.Network.Sequence;
class CharacterRow { public int TotalLogins; }
enum SequenceType { ObjectInstance }
class SequencesStub { public UShortSequence Value; public void SetSequence(SequenceType t,UShortSequence value) { Value=value; } }
class Program {
    CharacterRow Character=new(); SequencesStub Sequences=new();
    void Enter() {''' + statements + '''}
    static void Main() { foreach (int count in new[]{0,1,65534,65535,65536,131070,2147483646}) {
        var p=new Program();p.Character.TotalLogins=count;p.Enter();
        Console.WriteLine($"{count},{p.Character.TotalLogins},{p.Sequences.Value.CurrentValue}");
    }}
}''')
    (p/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
    dotnet='/tmp/bace-crafting-dotnet/dotnet'
    subprocess.run([dotnet,'build','--nologo','-o',str(p/'build')],cwd=p,check=True)
    output=subprocess.check_output([dotnet,str(p/'build/oracle.dll')],text=True)
(Path(__file__).resolve().parents[1]/'tests/fixtures/login_instance.csv').write_text(''.join('# '+str(p.relative_to(SOURCE))+' '+hashlib.sha256(p.read_bytes()).hexdigest()+'\n' for p in [player,sequence,interface])+output)
