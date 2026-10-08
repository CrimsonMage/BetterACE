#!/usr/bin/env python3
"""Compile unchanged official factory switch plus official subtype inheritance.
Only constructors and logging are replaced; source factory behavior is executable.
"""
from pathlib import Path
import re,json,tempfile,subprocess,sys,hashlib
root=Path(__file__).resolve().parents[5]
source=root/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
path=source/'ACE.Server/Factories/WorldObjectFactory.cs';factory=path.read_text()
a=factory.index('public static WorldObject CreateWorldObject(Weenie weenie, ObjectGuid guid)');b=factory.index('{',a)+1;depth=1
while depth:depth+=(factory[b]=='{')-(factory[b]=='}');b+=1
method=factory[a:b]
parents={}
for p in (source/'ACE.Server/WorldObjects').glob('*.cs'):
 for cls,parent in re.findall(r'class (\w+)\s*:\s*(\w+)',p.read_text()):parents[cls]=parent
classes=set(re.findall(r'return new (\w+)\(',method))
for _ in range(20):
 old=len(classes);classes.update(parents.get(c,'WorldObject') for c in list(classes) if c!='WorldObject')
 if len(classes)==old:break
classes.discard('WorldObject')
monster=(source/'ACE.Server/WorldObjects/Monster.cs').read_text()
a=monster.index('public void SetMonsterState()');b=monster.index('{',a)+1;depth=1
while depth:depth+=(monster[b]=='{')-(monster[b]=='}');b+=1
monster_method=monster[a:b]
classes.add('Player');parents['Player']='Creature'
code='''using System; using System.Text.Json; using ACE.Entity.Enum;
public record struct ObjectGuid(uint Full);
public class Weenie {public WeenieType WeenieType;public uint WeenieClassId=1;public string GetName()=>"fixture";}
public class WorldObject {public WeenieType WeenieType; public WorldObject(Weenie w,ObjectGuid g){WeenieType=w.WeenieType;}}
public enum TargetingTactic {None=0}
public class Logger {public void Warn(string value){}}
'''
for cls in sorted(classes):
 extra=''
 if cls=='Creature':extra='public bool Attackable,IsMonster,IsPassivePet,IsChessPiece,IsFactionMob,HasFoeType;public TargetingTactic TargetingTactic;public object Faction1Bits,FoeType;'+monster_method
 code+=f'public class {cls}:{parents.get(cls,"WorldObject")} {{ public {cls}(Weenie w,ObjectGuid g):base(w,g){{}} {extra} }}\n'
code+='public static class Factory {static Logger log=new();'+method+'}\n'
code+='''public static class Program {public static void Main(){var values=new System.Collections.Generic.List<object>();for(uint i=0;i<75;i++){var value=Factory.CreateWorldObject(new Weenie{WeenieType=(WeenieType)i},new ObjectGuid(1));values.Add(new {id=i,type=value?.GetType().Name,creature=value is Creature,container=value is Container});}var ai=new System.Collections.Generic.List<object>();foreach(var attackable in new[]{false,true})foreach(var tactic in new[]{0,1,2}){var vendor=new Vendor(new Weenie{WeenieType=WeenieType.Vendor},new ObjectGuid(1)){Attackable=attackable,TargetingTactic=(TargetingTactic)tactic};vendor.SetMonsterState();ai.Add(new {attackable,tactic,combat=vendor.IsMonster});}Console.WriteLine(JsonSerializer.Serialize(new {cases=values,ai_cases=ai}));}}'''
with tempfile.TemporaryDirectory(prefix='bace-factory-oracle-') as tmp:
 p=Path(tmp);(p/'Program.cs').write_text(code);(p/'WeenieType.cs').write_text((source/'ACE.Entity/Enum/WeenieType.cs').read_text());(p/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
 dotnet=sys.argv[1] if len(sys.argv)>1 else '/tmp/bace-crafting-dotnet/dotnet'
 result=subprocess.run([dotnet,'build','--nologo','-o',str(p/'out')],cwd=p,capture_output=True,text=True)
 if result.returncode:print(result.stdout);result.check_returncode()
 rows=json.loads(subprocess.run([dotnet,str(p/'out/oracle.dll')],cwd=p,capture_output=True,text=True,check=True).stdout)
 out=root/'crates/application/bace-runtime/tests/fixtures/factory_classes.json';out.write_text(json.dumps({'source_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'monster_sha256':hashlib.sha256(monster.encode()).hexdigest(),**rows},indent=2)+'\n');print(out)

 (root/'crates/simulation/bace-simulation/tests/fixtures/creature_combat_ai.csv').write_text('# Unchanged ACE Creature.SetMonsterState; Attackable|TargetingTactic|IsMonster\n'+'\n'.join(f"{int(v['attackable'])}|{v['tactic']}|{int(v['combat'])}" for v in rows['ai_cases'])+'\n')
