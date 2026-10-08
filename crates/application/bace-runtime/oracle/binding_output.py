#!/usr/bin/env python3
"""Run unchanged pinned Lifestone/Bindstone ActOnUse bodies with explicit adapters.

Qualifies call ordering, permission branches, position copies, and stamina rounding.
ActionChain, motion preparation, range, transport and SQL are deterministic stubs;
this does not qualify those helpers or complete binding gameplay.
AGPL-3.0-only; original methods copyright ACEmulator contributors.
"""
from pathlib import Path
import os
import hashlib
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[4]
PIN = '47edade3bd3f6044b676d4eb877c4965c7eda62b'
BASE = ROOT / '.reference' / ('ACE-' + PIN) / 'Source'


def method(text, signature):
    start = text.index(signature)
    end = text.index('{', start) + 1
    depth = 1
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    return text[start:end]


sources = [BASE / 'ACE.Server/WorldObjects' / (name + '.cs')
           for name in ['Lifestone', 'Bindstone']]
bodies = [method(path.read_text(), 'public override void ActOnUse(') for path in sources]
enums = []
for name, extra in [('CombatMode', ['Melee']), ('Sound', []), ('MotionCommand', []),
                    ('MotionStance', []), ('PropertyString', []),
                    ('ChatMessageType', []), ('WeenieError', []),
                    ('AllegiancePermissionLevel', [])]:
    original = next((BASE / 'ACE.Entity/Enum').rglob(name + '.cs')).read_text()
    members = sorted(set(re.findall(r'\b' + name + r'\.(\w+)', '\n'.join(bodies)) + extra))
    rows = []
    for member in members:
        value = re.search(r'^\s*' + member + r'\s*=\s*([^,\r\n]+)', original, re.M)
        assert value, (name, member)
        rows.append(member + '=' + value.group(1).strip())
    enums.append('public enum ' + name + ':uint{' + ','.join(rows) + '}')

program = r'''using System;using System.Collections.Generic;using System.Globalization;
ENUMS
static class Trace { public static List<string> Rows=new(); public static double Time;
 public static void Add(string s)=>Rows.Add(Time.ToString("R",CultureInfo.InvariantCulture)+":"+s); }
class Position { public double X; public Position(double x){X=x;} public Position(Position p){X=p.X;} }
class ActionChain { List<Action> actions=new(); public void AddAction(object owner,Action f)=>actions.Add(f);
 public void AddDelaySeconds(double seconds)=>actions.Add(()=>{Trace.Time+=seconds;});
 public void EnqueueChain(){foreach(var action in actions)action();} }
record Motion(MotionStance Stance,MotionCommand Command);
record GameMessageSound(uint Object,Sound Sound,float Volume){public override string ToString()=>"sound:"+(uint)Sound;}
record GameMessageSystemChat(string Text,ChatMessageType Type){public override string ToString()=>"chat:"+(uint)Type+":"+Text;}
record GameEventWeenieError(Session Session,WeenieError Error){public override string ToString()=>"error:"+(uint)Error;}
class Network {public void EnqueueSend(object message)=>Trace.Add(message.ToString());}
class Session {public Network Network=new();}
class Vital {public uint Current;}
class Allegiance {public Position Sanctuary;public void SaveBiotaToDatabase()=>Trace.Add("allegiance-save");}
class WorldObject {public virtual void ActOnUse(WorldObject o){} public string GetProperty(PropertyString p)=>"binding message";
 public void EnqueueBroadcastMotion(Motion m)=>Trace.Add("stone-motion:"+(uint)m.Command+":"+(uint)m.Stance);}
class Player:WorldObject {public Session Session=new();public uint Guid=1;public CombatMode CombatMode;
 public double LastUseTime;public bool InRange;public Position Location=new(5.5),Sanctuary;public Vital Stamina=new();
 public Allegiance Allegiance;public AllegiancePermissionLevel AllegiancePermissionLevel;
 public double SetCombatMode(CombatMode mode){CombatMode=mode;Trace.Add("mode:"+(uint)mode);return 1.25;}
 public void EnqueueBroadcast(object message)=>Trace.Add(message.ToString());
 public double EnqueueMotion(ActionChain chain,MotionCommand motion){chain.AddAction(this,()=>Trace.Add("player-motion:"+(uint)motion));chain.AddDelaySeconds(2.5);return 2.5;}
 public bool IsWithinUseRadiusOf(WorldObject o)=>InRange;
 public void UpdateVital(Vital vital,uint value){vital.Current=value;Trace.Add("stamina:"+value);} }
class Lifestone:WorldObject {LIFESTONE}
class Bindstone:WorldObject {BINDSTONE}
class Program {static void Main(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
 uint[] values={0,1,2,3,5,15,101,16777215,16777217,uint.MaxValue};
 for(int kind=0;kind<2;kind++)for(int combat=0;combat<2;combat++)for(int range=0;range<2;range++)for(int permission=0;permission<6;permission++)foreach(uint stamina in values){
 Trace.Rows.Clear();Trace.Time=0;var p=new Player{CombatMode=combat==0?CombatMode.NonCombat:CombatMode.Melee,InRange=range==1,Allegiance=permission==0?null:new(),AllegiancePermissionLevel=(AllegiancePermissionLevel)(permission-1)};p.Stamina.Current=stamina;
 WorldObject stone=kind==0?new Lifestone():new Bindstone();stone.ActOnUse(p);p.Location.X=99;
 Console.WriteLine($"{kind},{combat},{range},{permission},{stamina}\t{p.LastUseTime:R}\t{p.Sanctuary?.X.ToString("R")??"none"}\t{p.Allegiance?.Sanctuary?.X.ToString("R")??"none"}\t{string.Join("|",Trace.Rows)}");
 }}}
'''.replace('ENUMS', '\n'.join(enums)).replace('LIFESTONE', bodies[0]).replace('BINDSTONE', bodies[1])

with tempfile.TemporaryDirectory(prefix='bace-binding-output-') as tmp:
    directory = Path(tmp)
    (directory / 'Program.cs').write_text(program)
    (directory / 'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><WarningLevel>0</WarningLevel></PropertyGroup></Project>')
    dotnet = os.environ.get("BACE_DOTNET", "dotnet")
    subprocess.run([dotnet, 'build', '--nologo', '-o', str(directory / 'build')], cwd=directory, check=True)
    output = subprocess.check_output([dotnet, str(directory / 'build/oracle.dll')], text=True)
    provenance = '# official ACE ' + PIN + '\n'
    provenance += ''.join('# ' + path.name + ' sha256=' + hashlib.sha256(path.read_bytes()).hexdigest() + '\n' for path in sources)
    provenance += '# ActOnUse only; fixed helper stance=1.25s, animation=2.5s; no motion/range/SQL/transport qualification\n'
    provenance += '# kind,combat,in_range,permission,stamina<TAB>LastUseTime<TAB>player_copy<TAB>allegiance_copy<TAB>ordered calls\n'
    (Path(__file__).resolve().parents[1] / 'tests/fixtures/binding_output.tsv').write_text(provenance + output)
    print('original-source binding call/rounding vectors:', len(output.splitlines()))
