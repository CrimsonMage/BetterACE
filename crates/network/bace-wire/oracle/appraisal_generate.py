#!/usr/bin/env python3
"""Original ACE AppraiseInfo/profile serialization; excludes profile calculation."""
from pathlib import Path
import re,hashlib,tempfile,subprocess,os
ROOT=Path(__file__).resolve().parents[4];BASE=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Server/Network/Structure'
def block(text,marker):
 start=text.index(marker);end=text.index('{',start)+1;depth=1
 while depth:
  if text[end]=='{':depth+=1
  elif text[end]=='}':depth-=1
  end+=1
 return text[start:end]
names=['AppraiseInfo','ArmorProfile','CreatureProfile','WeaponProfile','HookProfile','ArmorLevel','HashComparer','PackableHashTable','PackableList']
sources={n:(BASE/(n+'.cs')).read_text() for n in names}
code='using System;using System.IO;using System.Collections.Generic;using System.Text;\n'
for name in ['PropertyInt','PropertyInt64','PropertyBool','PropertyFloat','PropertyString','PropertyDataId','PropertyInstanceId','ArmorMask','WeaponMask','ResistMask','AttributeMask','DamageType','Skill','HookFlags','EquipMask','AmmoType']:
 code+='public enum '+name+'{None=0}\n'
flags=(BASE.parents[2]/'ACE.Entity/Enum/IdentifyResponseFlags.cs').read_text()
code+='[Flags] '+block(flags,'public enum IdentifyResponseFlags')+'\n'
code+='[Flags] '+block(sources['CreatureProfile'],'public enum CreatureProfileFlags')+'\n'
for name in names[:6]:
 fields=re.findall(r'^\s*public (?:Dictionary<[^;]+>|List<uint>|[\w]+) [^\n\r;{}()]+;',sources[name],re.M)
 fields=[f for f in fields if 'WorldObject Weapon' not in f]
 code+='public class '+name+'{\n'+'\n'.join(fields)+'\n}\n'
for name in names[:6]:code+=block(sources[name],'public static class '+name+'Extensions')+'\n'
for name in ['Int','Int64','Bool','Float','String','DataId']:
 code+=block(sources['HashComparer'],'public class Property'+name+'Comparer')+'\n'
code+='static class PackableHashTable{'+block(sources['PackableHashTable'],'public static void WriteHeader(')+'}\n'
code+='static class PackableList{'+block(sources['PackableList'],'public static void Write(this BinaryWriter writer, List<uint> list)')+'}\n'
# Existing independently qualified string primitive. Keep ASCII fixture strings.
code+='''static class TextWriter {public static void WriteString16L(this BinaryWriter w,string value){byte[] b=Encoding.ASCII.GetBytes(value);w.Write((ushort)b.Length);w.Write(b);for(int i=0;i<(4-(b.Length+2)%4)%4;i++)w.Write((byte)0);}}
class Program {
static AppraiseInfo Fixture(uint flags,bool success,int creatureFlags){var p=new AppraiseInfo{Flags=(IdentifyResponseFlags)flags,Success=success,PropertiesInt=new(),PropertiesInt64=new(),PropertiesBool=new(),PropertiesFloat=new(),PropertiesString=new(),PropertiesDID=new(),SpellBook=new(){0x80001234,99}};
foreach(int id in new[]{17,8,1,16}){p.PropertiesInt.Add((PropertyInt)id,-id);p.PropertiesInt64.Add((PropertyInt64)id,-((long)id<<33));p.PropertiesBool.Add((PropertyBool)id,id%2==0);p.PropertiesFloat.Add((PropertyFloat)id,id/8.0);p.PropertiesString.Add((PropertyString)id,$"item{id}");p.PropertiesDID.Add((PropertyDataId)id,0xf0000000u+(uint)id);}
p.ArmorProfile=new(){SlashingProtection=1,PiercingProtection=2,BludgeoningProtection=3,ColdProtection=4,FireProtection=5,AcidProtection=6,NetherProtection=7,LightningProtection=8};
p.CreatureProfile=new(){Flags=(CreatureProfileFlags)creatureFlags,Health=7,HealthMax=100,Strength=1,Endurance=2,Quickness=3,Coordination=4,Focus=5,Self=6,Stamina=7,Mana=8,StaminaMax=9,ManaMax=10,AttributeHighlights=(AttributeMask)3,AttributeColors=(AttributeMask)1};
p.WeaponProfile=new(){DamageType=(DamageType)8,WeaponTime=25,WeaponSkill=(Skill)44,Damage=90,DamageVariance=.25,DamageMod=1.5,WeaponLength=2.5,MaxVelocity=30,WeaponOffense=1.25,MaxVelocityEstimated=1};p.HookProfile=new(){Flags=(HookFlags)3,ValidLocations=(EquipMask)0x100000,AmmoType=(AmmoType)2};
p.ArmorHighlight=(ArmorMask)3;p.ArmorColor=(ArmorMask)1;p.WeaponHighlight=(WeaponMask)5;p.WeaponColor=(WeaponMask)4;p.ResistHighlight=(ResistMask)9;p.ResistColor=(ResistMask)8;p.ArmorLevels=new(){Head=1,Chest=2,Abdomen=3,UpperArm=4,LowerArm=5,Hand=6,UpperLeg=7,LowerLeg=8,Foot=9};return p;}
static void Main(){var flags=new List<uint>{0,0x7fff};for(int i=0;i<15;i++)flags.Add(1u<<i);foreach(uint flag in flags)foreach(bool success in new[]{false,true})foreach(int c in new[]{0,1,8,9}){using var stream=new MemoryStream();using var writer=new BinaryWriter(stream);writer.Write(Fixture(flag,success,c));Console.WriteLine($"{flag}|{success}|{c}|{Convert.ToHexString(stream.ToArray())}");}}
}'''
with tempfile.TemporaryDirectory(prefix='bace-appraisal-') as directory:
 p=Path(directory);(p/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><WarningLevel>0</WarningLevel></PropertyGroup></Project>');(p/'Program.cs').write_text(code)
 result=subprocess.run([os.environ.get('DOTNET','/tmp/bace-crafting-dotnet/dotnet'),'run','--project',str(p/'Oracle.csproj'),'-c','Release'],text=True,capture_output=True)
 if result.returncode:raise RuntimeError(result.stdout+result.stderr)
 rows=[r for r in result.stdout.splitlines() if '|' in r];assert len(rows)==136
 provenance='\n'.join('# '+name+' '+hashlib.sha256(text.encode()).hexdigest() for name,text in sources.items())
 (ROOT/'crates/network/bace-wire/tests/fixtures/appraisal.txt').write_text('# ACE47edade3bd3f6044b676d4eb877c4965c7eda62b\n'+provenance+'\n'+'\n'.join(rows)+'\n');print(len(rows),'original appraisal serializer vectors')
