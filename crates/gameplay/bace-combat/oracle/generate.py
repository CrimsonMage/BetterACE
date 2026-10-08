#!/usr/bin/env python3
"""Compile verbatim official ACE methods with synthetic, deterministic adapters."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile
import urllib.request

PIN = "47edade3bd3f6044b676d4eb877c4965c7eda62b"
parser = argparse.ArgumentParser()
parser.add_argument("--source", type=Path, required=True)
parser.add_argument("--dotnet", default="dotnet")
a = parser.parse_args()
root = Path(__file__).resolve().parents[1]

def extract(text, signature):
    start = text.index(signature)
    opening = text.index("{", start)
    depth = 0
    for i in range(opening, len(text)):
        depth += (text[i] == "{") - (text[i] == "}")
        if depth == 0:
            return text[start:i + 1]
    raise ValueError(signature)

sources = {}
headers = [f"# Official ACEmulator/ACE {PIN}; synthetic oracle inputs, AGPL-3.0-only"]
for name in ["SkillCheck.cs", "Creature_Equipment.cs", "Vendor.cs"]:
    rel = f"Source/ACE.Server/WorldObjects/{name}"
    data = (a.source / rel).read_bytes()
    official = urllib.request.urlopen(f"https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{rel}", timeout=30).read()
    if data != official:
        raise ValueError(f"Source mismatch: {rel}")
    headers.append(f"# sha256 {hashlib.sha256(data).hexdigest()} {rel}")
    sources[name] = data.decode("utf-8-sig")

skill = extract(sources["SkillCheck.cs"], "public static double GetSkillChance(int")
loot = extract(sources["Creature_Equipment.cs"], "public static List<PropertiesCreateList> CreateListSelect(List<PropertiesCreateList> createList)")
sell = extract(sources["Vendor.cs"], "private uint GetSellCost(int? value, ItemType? itemType)")
buy = extract(sources["Vendor.cs"], "private int GetBuyCost(int? value, ItemType? itemType)")
harness = r'''
using System;
using System.Collections.Generic;
using System.Linq;
using System.Globalization;
[Flags] enum DestinationType { Contain=1, Wield=2, Treasure=8 }
enum ItemType { Other, PromissoryNote }
class PropertiesCreateList { public int Id; public uint DestinationType; public float Shade; }
static class PropertyManager { public static (double Item, int Other) GetDouble(string _) => (1.0, 0); }
static class ThreadSafeRandom {
 public static Queue<float> Draws = new();
 public static float Next(float lo,float hi) => Draws.Dequeue();
}
class Program {
 public double? SellPrice; public double? BuyPrice;
 // METHODS
 static List<PropertiesCreateList> CreateListSelect(List<PropertiesCreateList> x,float rate) => throw new Exception("non-default branch");
 static void Main() {
  CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
  foreach (int skill in new[]{0,1,50,100,150,300,1000})
   foreach (int difficulty in new[]{0,1,50,100,150,300,1000})
    Console.WriteLine($"skill,{skill},{difficulty},{GetSkillChance(skill,difficulty):R}");
  var p = new Program();
  foreach (int value in new[]{0,1,2,3,7,99,100,101,10000,16777217})
   foreach (double rate in new[]{0,0.1,0.333,0.5,1.0,1.15,1.25,2.0})
    foreach (bool note in new[]{false,true}) {
     p.SellPrice=rate; p.BuyPrice=rate;
     var type=note?ItemType.PromissoryNote:ItemType.Other;
     Console.WriteLine($"price,{value},{rate:R},{(note?1:0)},{p.GetSellCost(value,type)},{p.GetBuyCost(value,type)}");
    }
  var rows = new List<PropertiesCreateList> {
    new(){Id=0,DestinationType=1,Shade=0.7f},new(){Id=1,DestinationType=8,Shade=0.25f},
    new(){Id=2,DestinationType=8,Shade=0.75f},new(){Id=3,DestinationType=8,Shade=0},
    new(){Id=4,DestinationType=8,Shade=0.5f},new(){Id=5,DestinationType=8,Shade=0.5f},
    new(){Id=6,DestinationType=2,Shade=0.4f}};
  foreach(float first in new[]{0f,0.24999999f,0.25f,0.5f,0.99999994f})
   foreach(float second in new[]{0f,0.49999997f,0.5f,0.99999994f}) {
    ThreadSafeRandom.Draws=new Queue<float>(new[]{first,second});
    var selected=CreateListSelect(rows);
    Console.WriteLine($"loot,{first:R},{second:R},{string.Join(';',selected.Select(x=>x.Id))}");
   }
 }
}
'''
harness = harness.replace("// METHODS", "\n".join([skill, loot, sell, buy]))
with tempfile.TemporaryDirectory(prefix="bace-gameplay-oracle-") as td:
    build = Path(td)
    (build/"Oracle.csproj").write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>')
    (build/"Program.cs").write_text(harness)
    subprocess.run([a.dotnet,"build","--nologo","-o",str(build/"out"),str(build/"Oracle.csproj")],check=True)
    lines = subprocess.check_output([a.dotnet,str(build/"out/Oracle.dll")],text=True).splitlines()
    for prefix, crate, filename in [("skill,","bace-combat","skill.csv"),("price,","bace-economy","prices.csv"),("loot,","bace-loot","create-list.csv")]:
        out = root.parent/crate/"tests"/"fixtures"/filename
        out.parent.mkdir(parents=True,exist_ok=True)
        out.write_text("\n".join(headers+[x.removeprefix(prefix) for x in lines if x.startswith(prefix)])+"\n")
