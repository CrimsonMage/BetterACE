#!/usr/bin/env python3
"""Execute the unmodified corpse appearance/identity section of CreateCorpse.
The harness substitutes database/factory/physics boundary values only. No item
selection, decay, death animation, or wire parity is asserted by this fixture.
"""
from pathlib import Path
import hashlib,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[4]
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
path=ROOT/'.reference'/('ACE-'+PIN)/'Source/ACE.Server/WorldObjects/Creature_Death.cs'
s=path.read_text();a=s.index('            var cachedWeenie =',s.index('protected void CreateCorpse('));b=s.index('            bool saveCorpse = false;',a)
original=s[a:b]
program=r'''using System;using System.Collections.Generic;using System.Linq;
record G(uint Full);class Position {public Position ACEPosition()=>this;}
class Physics {public Position Position=new();}
class Biota {public List<int> PropertiesAnimPart=new(),PropertiesPalette=new(),PropertiesTextureMap=new();}
class ObjDesc {public List<int> AnimPartChanges=new(){17},SubPalettes=new(){18},TextureChanges=new(){19};}
static class Extensions {public static List<T> Clone<T>(this List<T> value,object gate)=>new(value);}
class Source {public G Guid=new(0x50000001);public string Name="Alice";public uint SetupTableId=11,MotionTableId=12,SoundTableId=13,PhysicsTableId=22;public uint? PaletteBaseDID=6,ClothingBase=7;public float? ObjScale,Shade;public int? PaletteTemplate;public Position Location;public uint? VictimId,KillerId;public string LongDesc;public Biota Biota=new();public object BiotaDatabaseLock=new();}
class Corpse:Source{public Corpse(){SetupTableId=101;MotionTableId=102;SoundTableId=103;PhysicsTableId=122;PaletteBaseDID=null;ClothingBase=null;Guid=new(0x80000001);}}
class Killer {public G Guid;public string Name;public Source PetOwner;public Source TryGetPetOwner()=>PetOwner;}
static class DatabaseManager {public static class World {public static int GetCachedWeenie(string name)=>1;}}
static class WorldObjectFactory {public static object CreateNewWorldObject(int cached)=>new Corpse();}
class Creature:Source {public bool TreasureCorpse;public Source Generator;public Physics PhysicsObj=new();public ObjDesc CalculateObjDesc()=>new();public Corpse Make(Killer killer){
ORIGINAL
return corpse;}}
class Program {static string V(object v)=>v?.ToString()??"-";static void Main(){for(int n=0;n<12;n++){var c=new Creature();if(n%2!=0){c.ObjScale=1.5f;c.Shade=.25f;c.PaletteTemplate=9;}if(n>=6)c.TreasureCorpse=true;Killer k=(n%6) switch{0=>null,1=>new(){Guid=new(0x50000002),Name="++Killer"},2=>new(){Guid=c.Guid,Name="Alice"},3=>new(){Guid=new(0x50000003),Name="  "},4=>new(){Guid=new(0x50000004),Name="Pet",PetOwner=new Source{Guid=new(0x50000005)}},_=>new(){Guid=new(0x50000006),Name="Generator"}};if(n%6==5)c.Generator=new Source{Guid=k.Guid};var p=c.Make(k);Console.WriteLine(string.Join("|",new object[]{n,p.Name,p.LongDesc,p.SetupTableId,p.MotionTableId,p.SoundTableId,V(p.PaletteBaseDID),V(p.ClothingBase),p.PhysicsTableId,V(p.ObjScale),V(p.PaletteTemplate),V(p.Shade),V(p.VictimId),V(p.KillerId),p.Biota.PropertiesAnimPart.Count,p.Biota.PropertiesPalette.Count,p.Biota.PropertiesTextureMap.Count}));}}}
'''.replace('ORIGINAL',original)
with tempfile.TemporaryDirectory(prefix='bace-corpse-source-') as tmp:
 d=Path(tmp);(d/'Program.cs').write_text(program);(d/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><WarningLevel>0</WarningLevel></PropertyGroup></Project>')
 dotnet='/tmp/bace-crafting-dotnet/dotnet';subprocess.run([dotnet,'build','--nologo','-o',str(d/'build')],cwd=d,check=True)
 out=subprocess.check_output([dotnet,str(d/'build/oracle.dll')],text=True)
 dest=Path(__file__).resolve().parents[1]/'tests/fixtures/player_corpse.trace'
 dest.write_text('# official ACE '+PIN+'\n# Creature_Death.cs sha256='+hashlib.sha256(path.read_bytes()).hexdigest()+'\n# scope: unmodified CreateCorpse appearance/identity block before player/loot handling\n'+out)
 print(len(out.splitlines()),'original-source corpse rows')
