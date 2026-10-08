#!/usr/bin/env python3
"""Execute all five unchanged ACE broadcast handlers; stub only surrounding services."""
from pathlib import Path
import hashlib, os, subprocess, tempfile
ROOT = Path(__file__).resolve().parents[4]
source = ROOT / '.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Server/Command/Handlers/AdminCommands.cs'
text = source.read_text()
names = ['HandleGamecast', 'HandleGameCastLocal', 'HandleGameCastEmote', 'HandleGameCastLocalEmote', 'HandleWe']
methods = []
for name in names:
    start = text.index('        public static void ' + name + '(')
    end = text.index('{', start) + 1
    depth = 1
    while depth:
        if text[end] == '{': depth += 1
        elif text[end] == '}': depth -= 1
        end += 1
    methods.append(text[start:end])
code = r'''using System;
enum ChatMessageType { WorldBroadcast=20 }
enum Channel { AllBroadcast }
class Player { public string Name="+Staff"; }
class Session { public Player Player=new(); }
class GameMessageSystemChat { public string Text; public ChatMessageType Type; public GameMessageSystemChat(string text, ChatMessageType type){Text=text;Type=type;} }
static class PlayerManager { public static GameMessageSystemChat Message; public static string Log;
 public static void BroadcastToAll(GameMessageSystemChat m){Message=m;}
 public static void LogBroadcastChat(Channel channel,Player sender,string text){Log=text;}
}
class Program {
''' + '\n'.join(methods) + r'''
static void Main(){
Action<Session,string[]>[] handlers={HandleGamecast,HandleGameCastLocal,HandleGameCastEmote,HandleGameCastLocalEmote,HandleWe};
for(int i=0;i<handlers.Length;i++)foreach(bool system in new[]{false,true})foreach(string text in new[]{"hello world",@"one\ntwo","éë test"}){
handlers[i](system?null:new Session(),new[]{text});
Console.WriteLine($"{i}|{system}|{Convert.ToHexString(System.Text.Encoding.UTF8.GetBytes(text))}|{Convert.ToHexString(System.Text.Encoding.UTF8.GetBytes(PlayerManager.Message.Text))}|{(int)PlayerManager.Message.Type}|{Convert.ToHexString(System.Text.Encoding.UTF8.GetBytes(PlayerManager.Log))}");
}}}'''
with tempfile.TemporaryDirectory(prefix='bace-staff-broadcast-') as directory:
    p=Path(directory)
    (p/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
    (p/'Program.cs').write_text(code)
    out=subprocess.check_output([os.environ.get('DOTNET','/tmp/bace-crafting-dotnet/dotnet'),'run','--project',str(p/'Oracle.csproj'),'-c','Release'],text=True)
    rows=[line for line in out.splitlines() if line[:2] in [f'{i}|' for i in range(5)]]
    assert len(rows)==30
    (ROOT/'crates/simulation/bace-simulation/tests/fixtures/staff_broadcast.txt').write_text('# ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b AdminCommands.cs sha256 '+hashlib.sha256(text.encode()).hexdigest()+'\n'+'\n'.join(rows)+'\n')
    print(len(rows),'original ACE broadcast cases')
