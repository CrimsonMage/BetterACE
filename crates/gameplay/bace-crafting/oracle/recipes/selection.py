#!/usr/bin/env python3
"""Compile original ACE GetNewRecipe tinkering switch cases + source recipe map.
Non-tinkering branches are intentionally not part of this adapter's claim.
"""
from pathlib import Path
import re,subprocess,tempfile,hashlib
ROOT=Path(__file__).resolve().parents[5]
SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
p=SRC/'ACE.Server/Managers/RecipeManager_New.cs';text=p.read_text(encoding='utf-8-sig')
start=text.index('                case WeenieClassName.W_MATERIALGOLD_CLASS:')
end=text.index('                // Society Shields',start)
cases=text[start:end]
mapstart=text.index('        public static Dictionary<WeenieClassName, uint> SourceToRecipe')
mapend=text.index('};',mapstart)+2
mapping=text[mapstart:mapend]
def enum(name):
 p=SRC/f'ACE.Entity/Enum/{name}.cs';s=p.read_text(encoding='utf-8-sig');start=s.index('public enum '+name);left=s.index('{',start);i=left+1;depth=1
 while depth:depth+=(s[i]=='{')-(s[i]=='}');i+=1
 return re.sub(r'\[[^\]\n]*\]','',s[start:i])
with tempfile.TemporaryDirectory(prefix='bace-tinker-selection-') as tmp:
 d=Path(tmp);(d/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 enums='\n'.join(enum(n) for n in ['WeenieClassName','WeenieType','MaterialType','ItemType','EquipMask'])
 code=enums+'''\nclass Recipe{public uint Id;public Recipe(uint id){Id=id;}}
class DatabaseManager{public static DatabaseManager World=new();public Recipe GetCachedRecipe(uint id)=>new(id);}
class WorldObject{public uint WeenieClassId;public WeenieType WeenieType;public float? Workmanship;public int? Value,EncumbranceVal,ItemMaxMana;public EquipMask ValidLocations;public ItemType ItemType;public MaterialType MaterialType;public bool Armor,IsEnchantable;public bool HasArmorLevel()=>Armor;}
class Selection{'''+mapping+'''\npublic static Recipe Select(WorldObject source,WorldObject target){Recipe recipe=null;switch((WeenieClassName)source.WeenieClassId){'''+cases+'''default:return null;}return recipe;}}
class Program{static void Main(){
'''
 names=re.findall(r'^\s*case WeenieClassName\.(\w+):',cases,re.M)
 code+='WeenieClassName[] ids={'+','.join('WeenieClassName.'+n for n in names)+'};\n'
 code+='''foreach(var id in ids)for(int n=0;n<24;n++){
var s=new WorldObject{WeenieClassId=(uint)id,MaterialType=n==23?MaterialType.Steel:0};
var t=new WorldObject{WeenieType=(WeenieType)(new int[]{1,2,3,6,35,5}[n%6]),Workmanship=n>=12?null:5,Value=n%3==0?0:10,EncumbranceVal=n%3==1?0:10,ItemMaxMana=n%3==2?0:10,ValidLocations=(EquipMask)(n%4==0?0x04000000:n%4==1?1:n%4==2?32:257),ItemType=(ItemType)(n%3==0?2:4),Armor=n%5!=0,IsEnchantable=n!=23};
Console.WriteLine($"{(uint)id}\\t{n}\\t{Selection.Select(s,t)?.Id??0}");
}}}'''
 (d/'Program.cs').write_text(code)
 dotnet='/tmp/bace-crafting-dotnet/dotnet';r=subprocess.run([dotnet,'build','--nologo','-o',str(d/'build')],cwd=d,capture_output=True,text=True)
 if r.returncode:print(r.stdout);r.check_returncode()
 r=subprocess.run([dotnet,str(d/'build/oracle.dll')],cwd=d,capture_output=True,text=True,check=True)
 dest=ROOT/'crates/gameplay/bace-crafting/tests/fixtures/tinker_selection.tsv';dest.write_text('# ACE 47edade3 original GetNewRecipe tinkering cases and SourceToRecipe\n# sha256 '+hashlib.sha256(p.read_bytes()).hexdigest()+' RecipeManager_New.cs\n'+r.stdout);print(dest)
