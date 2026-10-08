#!/usr/bin/env python3
"""Run pinned ACE recall admission/notice prefixes unchanged through SendMotion.
The delayed teleport continuation and actual network serialization are excluded.
AGPL-3.0-only; original method bodies copyright ACEmulator contributors.
"""
from pathlib import Path
import os
import hashlib, re, subprocess, tempfile
ROOT=Path(__file__).resolve().parents[4]
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
base=ROOT/'.reference'/('ACE-'+PIN)/'Source'
source=base/'ACE.Server/WorldObjects/Player_Location.cs'
text=source.read_text()
def method(signature):
    a=text.index(signature);i=text.index('{',a)+1;depth=1
    while depth:depth+=(text[i]=='{')-(text[i]=='}');i+=1
    return text[a:i]
names=['HandleActionTeleToLifestone','HandleActionTeleToHouse','HandleActionTeleToMarketPlace','HandleActionRecallAllegianceHometown','HandleActionTeleToMansion','HandleActionTeleToPkArena','HandleActionTeleToPklArena']
methods=[]
for name in names:
    body=method('public void '+name+'()')
    end=body.index(';',body.index('SendMotionAsCommands('))+1
    methods.append(body[:end]+'\n}')
methods.extend([method('private bool VerifyRecallAllegianceHometown()'),method('private House VerifyTeleToMansion()')])
methods='\n'.join(methods)
enums=[]
for name,extra in [('WeenieError',[]),('ChatMessageType',[]),('CombatMode',['Melee']),('PropertyInt',[]),('MotionStance',[]),('MotionCommand',[]),('HouseType',['Cottage']),('PlayerKillerStatus',['NPK','PK','PKLite'])]:
    path=next((base/'ACE.Entity/Enum').rglob(name+'.cs'));e=path.read_text()
    if name=='HouseType':
        enums.append(e[e.index('public enum HouseType'):e.rindex('}')].strip())
        continue
    members=sorted(set(re.findall(r'\b'+name+r'\.(\w+)',methods)+extra))
    rows=[]
    for member in members:
        match=re.search(r'^\s*'+member+r'\s*=\s*([^,\r\n]+)',e,re.M)
        assert match, (name,member)
        value=match.group(1).strip()
        rows.append(member+'='+value)
    enums.append('enum '+name+':uint{'+','.join(rows)+'}')
program=r'''using System;using System.Collections.Generic;
ENUMS
class Network { public List<string> Rows=new();public void EnqueueSend(object x)=>Rows.Add(x.ToString());}
class Session { public Network Network=new(); }
record GameEventWeenieError(Session Session,WeenieError Error){public override string ToString()=>"E:"+(uint)Error;}
record GameMessageSystemChat(string Text,ChatMessageType Type){public override string ToString()=>"C:"+(uint)Type+":"+Text;}
record GameMessagePrivateUpdatePropertyInt(Player Player,PropertyInt Property,int Value){public override string ToString()=>"P:"+(uint)Property+":"+Value;}
class Vital{public uint Current=101;}
class House{public HouseType HouseType=HouseType.Mansion;public uint? MonarchId=1;}
class Allegiance{public object Sanctuary=new();public House House=new();public House GetHouse()=>House;}
class Player{
public Session Session=new();public string Name="Recall Tester";public bool PKTimerActive,RecallsDisabled,TooBusyToRecall,IsOlthoiPlayer;
public CombatMode CombatMode=CombatMode.NonCombat;public PlayerKillerStatus PlayerKillerStatus=PlayerKillerStatus.PK;
public object Sanctuary=new();public Vital Mana=new();public House House=new();public Allegiance Allegiance=new();public int LocalBroadcastRange=192;
public House GetAccountHouse()=>null;
public void UpdateVital(Vital v,uint n){v.Current=n;Session.Network.Rows.Add("V:6:"+n);}
public void SetCombatMode(CombatMode m){CombatMode=m;}
public void EnqueueBroadcast(object message,int range,ChatMessageType type)=>Session.Network.EnqueueSend(message);
public void SendMotionAsCommands(MotionCommand motion,MotionStance stance)=>Session.Network.Rows.Add("M:"+(uint)motion+":"+(uint)stance);
METHODS
}
class Program{static void Main(){for(int kind=0;kind<7;kind++)for(int mode=0;mode<2;mode++)for(int scenario=0;scenario<13;scenario++){
var p=new Player();p.CombatMode=mode==0?CombatMode.NonCombat:CombatMode.Melee;p.PlayerKillerStatus=kind==6?PlayerKillerStatus.PKLite:PlayerKillerStatus.PK;
switch(scenario){case 1:p.PKTimerActive=true;break;case 2:p.RecallsDisabled=true;break;case 3:p.TooBusyToRecall=true;break;case 4:p.IsOlthoiPlayer=true;break;case 5:p.Sanctuary=null;break;case 6:p.House=null;break;case 7:p.Allegiance=null;break;case 8:p.Allegiance.Sanctuary=null;break;case 9:p.Allegiance.House=null;break;case 10:p.Allegiance.House.HouseType=HouseType.Cottage;break;case 11:p.Allegiance.House.MonarchId=null;break;case 12:p.PlayerKillerStatus=PlayerKillerStatus.NPK;break;}
new Action[]{CALLS}[kind]();Console.WriteLine($"{kind},{mode},{scenario}\t{string.Join("|",p.Session.Network.Rows)}");
}}}
'''.replace('ENUMS','\n'.join(enums)).replace('METHODS',methods).replace('CALLS',','.join('p.'+n for n in names))
with tempfile.TemporaryDirectory(prefix='bace-recall-output-') as tmp:
    d=Path(tmp);(d/'Program.cs').write_text(program);(d/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><WarningLevel>0</WarningLevel></PropertyGroup></Project>')
    dotnet = os.environ.get("BACE_DOTNET", "dotnet")
    subprocess.run([dotnet,'build','--nologo','-o',str(d/'build')],cwd=d,check=True)
    output=subprocess.check_output([dotnet,str(d/'build/oracle.dll')],text=True)
    (Path(__file__).resolve().parents[1]/'tests/fixtures/recall_output.tsv').write_text('# official ACE '+PIN+'\n# Player_Location.cs sha256='+hashlib.sha256(source.read_bytes()).hexdigest()+'\n# kind,combat,scenario<TAB>ordered notice trace; delayed continuation excluded\n'+output)
    print('original-source recall admission/notice vectors:',len(output.splitlines()))
