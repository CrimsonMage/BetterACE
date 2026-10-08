#!/usr/bin/env python3
"""Original ACE Vendor.AddDefaultItem retaining constructed container contents."""
from pathlib import Path
import hashlib
import subprocess
import tempfile
ROOT=Path(__file__).resolve().parents[4]
source=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Server/WorldObjects/Vendor.cs'
text=source.read_text()
def method(signature):
    start=text.index(signature); brace=text.index('{',start); depth=1; end=brace+1
    while depth:
        depth+=(text[end]=='{')-(text[end]=='}');end+=1
    return text[start:end]
body=method('public void AddDefaultItem(WorldObject item)')+method('public List<WorldObject> GetDefaultItemsByWcid(uint wcid)')
with tempfile.TemporaryDirectory(prefix='bace-vendor-tree-') as directory:
    p=Path(directory)
    (p/'Program.cs').write_text('''
using System;using System.Collections.Generic;using System.Linq;
record ObjectGuid(uint Full);
class WorldObject { public ObjectGuid Guid; public uint WeenieClassId=100; public int? StackSize,MaxStackSize; public uint? ContainerId;
    public Dictionary<uint,WorldObject> Inventory=new(); public void CalculateObjDesc(){} public void SetStackSize(int size){StackSize=size;} }
class Vendor:WorldObject { public Dictionary<ObjectGuid,WorldObject> DefaultItemsForSale=new();''' + body + '''}
class Program { static void Main() {
    foreach (int maximum in new[]{1,100}) {
        var v=new Vendor{Guid=new ObjectGuid(900)};
        var a=new WorldObject{Guid=new ObjectGuid(1),StackSize=maximum==1?1:5,MaxStackSize=maximum};
        var b=new WorldObject{Guid=new ObjectGuid(2),StackSize=maximum==1?1:99,MaxStackSize=maximum};
        a.Inventory.Add(101,new WorldObject{Guid=new ObjectGuid(101)}); b.Inventory.Add(102,new WorldObject{Guid=new ObjectGuid(102)});
        v.AddDefaultItem(a);v.AddDefaultItem(b);
        Console.WriteLine($"{maximum}|{string.Join(',',v.DefaultItemsForSale.Values.Select(o=>o.Guid.Full))}|{string.Join(',',v.DefaultItemsForSale.Values.Select(o=>o.StackSize))}|{string.Join(',',v.DefaultItemsForSale.Values.SelectMany(o=>o.Inventory.Keys))}|{string.Join(',',v.DefaultItemsForSale.Values.Select(o=>o.ContainerId))}");
    }
}}''')
    (p/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
    dotnet='/tmp/bace-crafting-dotnet/dotnet'
    subprocess.run([dotnet,'build','--nologo','-o',str(p/'build')],cwd=p,check=True)
    output=subprocess.check_output([dotnet,str(p/'build/oracle.dll')],text=True)
(Path(__file__).resolve().parents[1]/'tests/fixtures/vendor_tree.csv').write_text('# ACE.Server/WorldObjects/Vendor.cs '+hashlib.sha256(source.read_bytes()).hexdigest()+'\n# max_stack|roots|stacks|children|vendor\n'+output)
