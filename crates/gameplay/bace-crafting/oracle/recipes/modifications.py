#!/usr/bin/env python3
"""Pinned RecipeManager.VerifyRequirement/Modify* unchanged C# method bodies."""
from pathlib import Path
import subprocess,tempfile,re,sys,hashlib
ROOT=Path(__file__).resolve().parents[5];SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
source=(SRC/'ACE.Server/Managers/RecipeManager.cs').read_text()
def extract(text,sig):
 start=text.index(sig);i=text.index('{',start)+1;depth=1
 while depth:depth+=(text[i]=='{')-(text[i]=='}');i+=1
 return text[start:i]
methods='\n'.join(extract(source,s) for s in ['public static bool VerifyRequirement(Player player, CompareType compareType, double?', 'public static bool VerifyRequirement(Player player, CompareType compareType, string', 'public static WorldObject GetSourceMod(', 'public static WorldObject GetTargetMod(', 'public static void ModifyBool(', 'public static void ModifyInt(', 'public static void ModifyFloat(', 'public static void ModifyString(', 'public static void ModifyInstanceID(', 'private static uint ModifyInstanceIDRuleSet(', 'public static void ModifyDataID('])
with tempfile.TemporaryDirectory(prefix='bace-recipe-mod-oracle-') as tmp:
 out=Path(tmp);(out/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>')
 enums='namespace ACE.Entity.Enum {'+''.join(extract((SRC/f'ACE.Entity/Enum/{n}.cs').read_text(),'public enum '+n) for n in ['ModificationType','ModificationOperation','RecipeSourceType','CompareType'])+'}\nnamespace ACE.Entity.Enum.Properties {'+''.join(re.sub(r'\[[^\]\n]*\]','',extract((SRC/f'ACE.Entity/Enum/Properties/Property{n}.cs').read_text(),'public enum Property'+n)) for n in ['Int','Float','Bool','String','InstanceId','DataId'])+'}'
 (out/'Enums.cs').write_text(enums)
 rows=''.join(f'class RecipeMods{n} {{public sbyte Index; public int Stat,Source,Enum; public {t} Value;}}' for n,t in [('Bool','bool'),('Int','int'),('Float','double'),('String','string'),('IID','uint'),('DID','uint')])
 (out/'Program.cs').write_text((Path(__file__).parent/'modifications.cs').read_text().replace('/*ROW_TYPES*/',rows).replace('/*METHODS*/',methods))
 dotnet=sys.argv[1] if len(sys.argv)>1 else '/tmp/bace-crafting-dotnet/dotnet';p=subprocess.run([dotnet,'build','--nologo','-o',str(out/'build')],cwd=out,capture_output=True,text=True)
 if p.returncode:print(p.stdout);p.check_returncode()
 p=subprocess.run([dotnet,str(out/'build/oracle.dll')],cwd=out,capture_output=True,text=True,check=True)
 dest=ROOT/'crates/gameplay/bace-crafting/tests/fixtures/recipe_modifications.tsv';dest.write_text('# ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b RecipeManager.cs sha256 '+hashlib.sha256((SRC/'ACE.Server/Managers/RecipeManager.cs').read_bytes()).hexdigest()+'\n'+p.stdout);print(dest)
