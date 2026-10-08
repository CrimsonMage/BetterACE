#!/usr/bin/env python3
"""Independent original C# attribute constructors, reflection and parser oracle.
Requires .NET8; no Rust implementation is read or invoked.
"""
from pathlib import Path
import sys,tempfile,subprocess,json,re,hashlib
ROOT=Path(__file__).resolve().parents[4]
SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
sys.path.insert(0,str(Path(__file__).parent))
# Reuse lexical boundaries ONLY, not metadata interpretation.
from generate_commands import attributes,clean
with tempfile.TemporaryDirectory(prefix='bace-command-oracle-') as tmp:
 out=Path(tmp)
 (out/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings><EnableDefaultCompileItems>true</EnableDefaultCompileItems></PropertyGroup></Project>')
 for name,path in [('AccessLevel.cs','ACE.Entity/Enum/AccessLevel.cs'),('Attribute.cs','ACE.Server/Command/CommandHandlerAttribute.cs'),('Flag.cs','ACE.Server/Command/CommandHandlerFlag.cs')]:
  (out/name).write_bytes((SRC/path).read_bytes())
 classes=[]
 for index,f in enumerate(sorted((SRC/'ACE.Server').rglob('*.cs'))):
  s=clean(f.read_text(encoding='utf-8-sig'));attrs=list(attributes(s))
  if not attrs:continue
  body=['using ACE.Entity.Enum;using ACE.Server.Command;','public class Source'+str(index)+' {']
  for ordinal,(start,end,raw) in enumerate(attrs):body+=['[CommandHandler('+raw+')]','public static void Method'+str(ordinal)+'(){}']
  body+=['}'];(out/(str(index)+'.cs')).write_text('\n'.join(body));classes.append((index,str(f.relative_to(SRC/'ACE.Server'))))
 source=(SRC/'ACE.Server/Command/CommandManager.cs').read_text()
 start=source.index('        public static void ParseCommand(');end=source.index('        public static CommandHandlerResponse GetCommandHandler(',start)
 (out/'Parser.cs').write_text('using System;using System.Collections.Generic;public static class Parser {\n'+source[start:end]+'\n}')
 inputs=['@rename "Some Person" "New Name"','/heal','  @teleto    "A B"  ','@set-accountpassword a "quoted password"','@gamecast "hello, world"','heal','@teleloc 0x12340001 1 2 3','@x "one" two','@x abc"def','@x ""','@x "two  spaces"']
 encoded=json.dumps(inputs)
 program='''using System.Reflection;using System.Text.Json;using ACE.Server.Command;
var rows=new List<object>();
foreach(var type in Assembly.GetExecutingAssembly().GetTypes().Where(t=>t.Name.StartsWith("Source")).OrderBy(t=>int.Parse(t.Name.Substring(6))))
foreach(var method in type.GetMethods().Where(m=>m.Name.StartsWith("Method")).OrderBy(m=>int.Parse(m.Name.Substring(6))))
foreach(var a in method.GetCustomAttributes<CommandHandlerAttribute>()) rows.Add(new { name=a.Command,access=(int)a.Access,flags=(int)a.Flags,min_args=a.ParameterCount,include_raw=a.IncludeRaw,description=a.Description,usage=a.Usage });
var inputs=JsonSerializer.Deserialize<string[]>(INPUTJSON)!;
var parses=new List<object>();foreach(var input in inputs){Parser.ParseCommand(input,out var name,out var arguments);parses.Add(new {input,name,arguments});}
Console.WriteLine(JsonSerializer.Serialize(new {declarations=rows,parses}));
'''.replace('INPUTJSON',json.dumps(encoded))
 (out/'Program.cs').write_text(program)
 subprocess.run([sys.argv[1] if len(sys.argv)>1 else '/tmp/bace-crafting-dotnet/dotnet','build','--nologo','-o',str(out/'build')],cwd=out,check=True,stdout=subprocess.DEVNULL)
 run=subprocess.run([sys.argv[1] if len(sys.argv)>1 else '/tmp/bace-crafting-dotnet/dotnet',str(out/'build/oracle.dll')],capture_output=True,text=True,check=True)
 fixture=Path(__file__).resolve().parents[1]/'tests/fixtures/commands.json';fixture.parent.mkdir(exist_ok=True);fixture.write_text(json.dumps(json.loads(run.stdout),indent=2)+'\n')
 print(fixture)
