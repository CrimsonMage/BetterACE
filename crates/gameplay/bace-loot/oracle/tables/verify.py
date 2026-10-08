#!/usr/bin/env python3
"""Compile original C# field initializers and enums, reflect independent fixtures.
No Python-evaluated enum/probability values enter the C# oracle.
"""
from pathlib import Path
import re, subprocess, argparse, tempfile, json
from generate import ROOT, PIN, clean, body
parser=argparse.ArgumentParser()
parser.add_argument('--dotnet',default='/tmp/bace-crafting-dotnet/dotnet')
args=parser.parse_args()
source=ROOT/'.reference'/('ACE-'+PIN)/'Source'
pattern=re.compile(r'(?:private|public|internal)\s+static\s+(?:readonly\s+)?(?:ChanceTable<[^;=]+?>|List<[^;=]+?>|Dictionary<[^;=]+?>)\s+(\w+)\s*=\s*new\s+[^;{]+\{')
classes=[];all_fields=[]
for p in sorted(list((source/'ACE.Server/Factories/Tables').rglob('*.cs'))+[source/'ACE.Server/Factories/Entity/MissileMagicDefense.cs']):
    text=clean(p.read_text(encoding='utf-8-sig'));name=re.search(r'\bclass\s+(\w+)',text)[1];fields=[]
    for m in pattern.finditer(text):
        # Empty dictionaries are populated by methods, not static authored data.
        field=text[m.start():m.end()]+body(text,m.end())+'};'
        if 'Dictionary<' in field and m[1] not in ('ValueMod','descriptors'):continue
        fields.append(field);all_fields.append(field)
    if fields:classes.append('public static class '+name+' {\n'+'\n'.join(fields)+'\n}')
needed=set(re.findall(r'\b(\w+)\.\w+', '\n'.join(all_fields)))
needed.update(('WeenieClassName','MaterialType'))
enums=[]
for p in source.rglob('*.cs'):
    if '/Enum/' not in str(p):continue
    text=clean(p.read_text(encoding='utf-8-sig'))
    for m in re.finditer(r'\benum\s+(\w+)(?:\s*:\s*\w+)?\s*\{',text):
        if m[1] in needed:
            enum=text[m.start():m.end()]+body(text,m.end())+'}'
            enum=re.sub(r'\[[^\]]*\]','',enum)
            enums.append('public '+enum);needed.remove(m[1])
header='using System; using System.Collections; using System.Collections.Generic; using System.Reflection; using System.Linq;\n'
support='public class ChanceTable<T>:List<(T,float)> {}\npublic class GemResult {public WeenieClassName ClassName; public MaterialType MaterialType; public GemResult(WeenieClassName c,MaterialType m){ClassName=c;MaterialType=m;}}\n'
harness=r'''
public class Program {
 static long Numeric(object v) => Convert.ToInt64(v);
 public static void Main() {
 var types=Assembly.GetExecutingAssembly().GetTypes().Where(t=>t.IsAbstract&&t.IsSealed).OrderBy(t=>t.Name);
 var names=new Dictionary<object,string>(ReferenceEqualityComparer.Instance);
 foreach(var t in types) foreach(var f in t.GetFields(BindingFlags.Static|BindingFlags.Public|BindingFlags.NonPublic)) {
   var value=f.GetValue(null);if(value!=null)names.TryAdd(value,t.Name+"."+f.Name);
 }
 foreach(var t in types) foreach(var f in t.GetFields(BindingFlags.Static|BindingFlags.Public|BindingFlags.NonPublic)) {
   var value=f.GetValue(null);if(value is not IEnumerable rows)continue;
   var key=t.Name+"."+f.Name;
   foreach(var row in rows) {
     if(row==null){Console.WriteLine($"reference|{key}|null");continue;}
     if(row is System.Runtime.CompilerServices.ITuple tuple) {
       if(tuple[0] is GemResult gem)Console.WriteLine($"gem|{key}|{Numeric(gem.ClassName)}|{Numeric(gem.MaterialType)}|{BitConverter.SingleToUInt32Bits((float)tuple[1])}");
       else if(names.TryGetValue(tuple[0],out var name))Console.WriteLine($"typed|{key}|{name}|{Numeric(tuple[1])}");
       else Console.WriteLine($"chance|{key}|{(tuple[0] is float x ? BitConverter.SingleToUInt32Bits(x) : Numeric(tuple[0]))}|{BitConverter.SingleToUInt32Bits((float)tuple[1])}");
     } else if(names.TryGetValue(row,out var reference))Console.WriteLine($"reference|{key}|{reference}");
     else if(row is float x)Console.WriteLine($"float|{key}|{BitConverter.SingleToUInt32Bits(x)}");
     else if(row.GetType().IsGenericType && row.GetType().GetGenericTypeDefinition()==typeof(KeyValuePair<,>)) {
        var k=row.GetType().GetProperty("Key").GetValue(row);var v=row.GetType().GetProperty("Value").GetValue(row);
        Console.WriteLine(v is float y ? $"chance|{key}|{Numeric(k)}|{BitConverter.SingleToUInt32Bits(y)}" : $"descriptor|{key}|{Numeric(k)}|{v}");
     } else Console.WriteLine($"sequence|{key}|{Numeric(row)}");
   }
 }
 }
}
'''
with tempfile.TemporaryDirectory(prefix='bace-table-oracle-') as directory:
    d=Path(directory)
    (d/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><EnableNETAnalyzers>false</EnableNETAnalyzers><NoWarn>0169;0414;0649</NoWarn></PropertyGroup></Project>')
    (d/'Program.cs').write_text(header+'\n'.join(enums)+support+'\n'.join(classes)+harness)
    result=subprocess.run([args.dotnet,'run','--project',str(d/'oracle.csproj'),'-c','Release'],capture_output=True,text=True)
    if result.returncode:
        print(result.stdout[-12000:]+result.stderr[-2000:]);raise SystemExit(result.returncode)
    rows=[r for r in result.stdout.splitlines() if '|' in r]
    target=ROOT/'crates/gameplay/bace-loot/tests/fixtures/ace-tables.csv'
    target.write_text('# Original compiled C# field initializers and enums, ACE '+PIN+'\n'+'\n'.join(rows)+'\n')
    print(f'{len(rows)} original C# rows -> {target}')
