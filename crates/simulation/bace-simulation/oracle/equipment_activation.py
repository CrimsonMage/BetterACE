#!/usr/bin/env python3
"""Run unchanged pinned Player.TryActivateSpells with only surrounding services stubbed."""
from pathlib import Path
import tempfile,subprocess,os,hashlib
ROOT=Path(__file__).resolve().parents[4]
source=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Server/WorldObjects/Player_Inventory.cs'
text=source.read_text();start=text.index('        private bool TryActivateSpells(');begin=text.index('{',start);end=begin+1;depth=1
while depth:
    if text[end]=='{':depth+=1
    elif text[end]=='}':depth-=1
    end+=1
method=text[start:end]
code='''using System; using System.Collections.Generic;
class Result {public bool Success;public object Message;}
class Network {public void EnqueueSend(object m){}}
class Session {public Network Network=new();}
class Biota {public List<int> Spells=new(){1,2,3};public List<int> GetKnownSpellsIds(object l)=>Spells;}
class WorldObject {public int? ItemCurMana;public bool Active;public bool Allowed;public uint? SpellDID=3;public Biota Biota=new();public object BiotaDatabaseLock=new();public Result CheckUseRequirements(Player p)=>new(){Success=Allowed};public bool HasProcSpell(uint id)=>id==2;public void OnSpellsActivated(){Active=true;}}
class Player {public object BiotaDatabaseLock=new();public Session Session=new();public int Casts;public bool SpellSuccess;public bool CreateItemSpell(WorldObject item,uint id){Casts++;return SpellSuccess;}
'''+method+'''
public static void Main(){foreach(int mana in new[]{1,2,20})foreach(bool active in new[]{false,true})foreach(bool allowed in new[]{false,true})foreach(bool success in new[]{false,true})foreach(bool empty in new[]{false,true}){var p=new Player{SpellSuccess=success};var item=new WorldObject{ItemCurMana=mana,Active=active,Allowed=allowed};if(empty)item.Biota.Spells.Clear();var result=p.TryActivateSpells(item);Console.WriteLine($"{mana} {active} {allowed} {success} {empty} {item.ItemCurMana} {item.Active} {p.Casts} {result}");}}
}'''
with tempfile.TemporaryDirectory(prefix='bace-equipment-oracle-') as d:
    p=Path(d);(p/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>');(p/'Program.cs').write_text(code)
    out=subprocess.check_output([os.environ.get('DOTNET','/tmp/bace-crafting-dotnet/dotnet'),'run','--project',str(p/'Oracle.csproj'),'-c','Release'],text=True)
    rows=[line for line in out.splitlines() if line.split(' ',1)[0] in ('1','2','20')]
    target=ROOT/'crates/simulation/bace-simulation/tests/fixtures/equipment_activation.txt'
    target.write_text('# ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b Player_Inventory.cs sha256 '+hashlib.sha256(text.encode()).hexdigest()+'\n'+'\n'.join(rows)+'\n')
    print(len(rows),'original ACE activation vectors')
