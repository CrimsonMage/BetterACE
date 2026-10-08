#!/usr/bin/env python3
"""Unchanged pinned equipment-set methods; deterministic object/DAT stand-ins."""
from pathlib import Path
import tempfile,subprocess,os,hashlib
ROOT=Path(__file__).resolve().parents[4]
ACE=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Server/WorldObjects'
p=(ACE/'Player_Spells.cs').read_text();w=(ACE/'WorldObject_Set.cs').read_text()
def method(source,name):
    start=source.rfind('\n',0,source.index(name))+1;begin=source.index('{',start);end=begin+1;depth=1
    while depth:
        if source[end]=='{':depth+=1
        elif source[end]=='}':depth-=1
        end+=1
    return source[start:end]
methods='\n'.join(method(p,name) for name in ['public void EquipItemFromSet(','public void EquipDequipItemFromSet(','public void DequipItemFromSet('])
code='''using System;using System.Collections.Generic;using System.Linq;
class Spell:IEquatable<Spell>{public uint Id;public string Name=>Id.ToString();public Spell(uint id,bool unused=false){Id=id;}public bool Equals(Spell other)=>other!=null&&Id==other.Id;public override bool Equals(object o)=>o is Spell s&&Equals(s);public override int GetHashCode()=>Id.GetHashCode();}
class Tier{public List<uint> Spells;public Tier(params uint[] ids){Spells=ids.ToList();}}
class Set{public uint HighestTier=6;public Dictionary<uint,Tier> SpellSetTiersNoGaps=new(){[0]=new(),[1]=new(100),[2]=new(200,900),[3]=new(300,900),[4]=new(400,900),[5]=new(500,900),[6]=new(600,900)};}
class Table{public Dictionary<uint,Set> SpellSet=new(){[1]=new()};}class Portal{public Table SpellTable=new();}static class DatManager{public static Portal PortalDat=new();}
class Registry{public List<string> Log;public Spell GetEnchantment(uint id,uint set)=>new(id);public void Dispel(Spell s){Log.Add("-"+s.Id);}}
static class log{public static void Error(string s){throw new Exception(s);}}
class WorldObject{public uint Id;public string Name=>Id.ToString();public bool HasItemSet=>true;public uint? EquipmentSetId=1;public int? ItemXpStyle;public int? ItemLevel;
'''+method(w,'public static List<Spell> GetSpellSet(')+'''
}
class Player:WorldObject{public Dictionary<uint,WorldObject> EquippedObjects=new();public Registry EnchantmentManager;public List<string> Events=new();public Player(){EnchantmentManager=new(){Log=Events};}public bool CreateItemSpell(WorldObject item,uint spell){Events.Add("+"+spell+"@"+item.Id);return true;}
'''+methods+'''
static WorldObject Item(int id,int style)=>new(){Id=(uint)id,ItemXpStyle=style,ItemLevel=id};
public static void Main(){foreach(int style in new[]{0,1})for(int count=1;count<=4;count++){var e=new Player();for(int id=1;id<=count;id++)e.EquippedObjects[(uint)id]=Item(id,style);e.EquipItemFromSet(e.EquippedObjects[(uint)count]);Console.WriteLine($"equip {style} {count} {count} {string.Join(",",e.Events)}");for(int removed=1;removed<=count;removed++){var d=new Player();for(int id=1;id<=count;id++)if(id!=removed)d.EquippedObjects[(uint)id]=Item(id,style);d.DequipItemFromSet(Item(removed,style));Console.WriteLine($"dequip {style} {count} {removed} {string.Join(",",d.Events)}");}}}}
'''
with tempfile.TemporaryDirectory(prefix='bace-equipment-set-oracle-') as d:
    path=Path(d);(path/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>');(path/'Program.cs').write_text(code)
    result=subprocess.run([os.environ.get('DOTNET','/tmp/bace-crafting-dotnet/dotnet'),'run','--project',str(path/'Oracle.csproj'),'-c','Release'],capture_output=True,text=True)
    if result.returncode:raise RuntimeError(result.stdout+result.stderr)
    rows=[line for line in result.stdout.splitlines() if line.startswith(('equip ','dequip '))]
    (ROOT/'crates/simulation/bace-simulation/tests/fixtures/equipment_sets.txt').write_text('# ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b Player_Spells.cs sha256 '+hashlib.sha256(p.encode()).hexdigest()+' WorldObject_Set.cs sha256 '+hashlib.sha256(w.encode()).hexdigest()+'\n'+'\n'.join(rows)+'\n')
    print(len(rows),'original ACE equipment-set vectors')
