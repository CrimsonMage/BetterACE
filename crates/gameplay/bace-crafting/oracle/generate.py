#!/usr/bin/env python3
"""Execute pinned source methods with synthetic adapters, never Rust formulas."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile

ACE = "47edade3bd3f6044b676d4eb877c4965c7eda62b"
GDLE = "353cbab52ef7da2b7063bc3e3f008461d8531693"
p = argparse.ArgumentParser()
p.add_argument("--ace", type=Path, required=True)
p.add_argument("--gdle", type=Path, required=True)
p.add_argument("--dotnet", required=True)
a = p.parse_args()
root = Path(__file__).resolve().parents[1]

def extract(source, signature):
    begin = source.index(signature)
    opening = source.index("{", begin)
    depth = 0
    for end in range(opening, len(source)):
        depth += (source[end] == "{") - (source[end] == "}")
        if depth == 0:
            return source[begin:end + 1]
    raise ValueError(signature)

def pinned(path, pin, relative):
    # Read the immutable Git object when a checkout is supplied. An exported
    # source tree must match that same object from the local official repository.
    repo = path if (path / ".git").exists() else Path("/home/jokerfactor/Documents/GitHub/ACE")
    data = subprocess.check_output(["git", "-C", str(repo), "show", f"{pin}:{relative}"])
    if (path / relative).read_bytes() != data:
        raise ValueError(f"source differs from pinned object: {relative}")
    return data.decode("utf-8-sig"), f"# {pin} sha256 {hashlib.sha256(data).hexdigest()} {relative}\n"

recipe, rh = pinned(a.ace, ACE, "Source/ACE.Server/Managers/RecipeManager.cs")
skill, sh = pinned(a.ace, ACE, "Source/ACE.Server/WorldObjects/SkillCheck.cs")
material, mh = pinned(a.ace, ACE, "Source/ACE.Entity/Enum/MaterialType.cs")
method = extract(recipe, "public static double? GetTinkerChance(")
# Instrument only the local difficulty result. Preserve upstream expressions.
method = method.replace("var successChance = SkillCheck.GetSkillChance", "LastDifficulty = difficulty;\n            var successChance = SkillCheck.GetSkillChance")
material = material.replace("namespace ACE.Entity.Enum", "namespace Oracle")
harness = r'''
using System;
using System.Collections.Generic;
using System.Globalization;
using Oracle;
enum Skill { Any }
enum SkillAdvancementClass { Untrained, Trained, Specialized }
enum ChatMessageType { Broadcast }
enum WeenieClassName : uint { Normal, Foolproof }
class GameMessageSystemChat { public GameMessageSystemChat(string s, ChatMessageType t) {} }
class Network { public void EnqueueSend(object v) {} }
class Session { public Network Network = new(); }
class CreatureSkill { public Skill Skill = Skill.Any; public uint Current; public SkillAdvancementClass AdvancementClass = SkillAdvancementClass.Trained; }
class Player { public int LumAugSkilledCraft; public int AugmentationBonusImbueChance; public CreatureSkill Ability = new(); public Session Session = new(); public CreatureSkill GetCreatureSkill(Skill s) => Ability; }
class WorldObject { public float? Workmanship; public int NumTimesTinkered; public MaterialType? MaterialType; public uint WeenieClassId; }
class Recipe { public int Skill; public bool Imbue; public bool IsImbuing() => Imbue; }
static class Extensions { public static string ToSentence(this Skill s) => "skill"; }
static class SkillCheck { /*SKILL*/ }
class Program {
 public static int LastDifficulty;
 public static HashSet<WeenieClassName> foolproofTinkers = new(){WeenieClassName.Foolproof};
 public static List<float> TinkeringDifficulty = new(){1f,1.1f,1.3f,1.6f,2f,2.5f,3f,3.5f,4f,4.5f};
 /*METHOD*/
 /*MODIFIER*/
 static void Main() {
  CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
  for(uint mat=1;mat<=77;mat++) {
   for(int attempt=0;attempt<10;attempt++) {
    float tool=attempt%3==0?5f:attempt%3==1?9.99f:10f;
    float item=10f;
    uint level=(uint)(attempt*53);
    var player=new Player {LumAugSkilledCraft=attempt%2==0?0:20,Ability=new(){Current=level}};
    var source=new WorldObject {Workmanship=tool,MaterialType=(MaterialType)mat,WeenieClassId=0};
    var target=new WorldObject {Workmanship=item,NumTimesTinkered=attempt};
    var recipe=new Recipe {Skill=0,Imbue=false};
    var chance=GetTinkerChance(player,source,target,recipe);
    Console.WriteLine($"{mat},{level},{tool:R},{item:R},{attempt},{player.LumAugSkilledCraft},{LastDifficulty},{chance:R}");
   }
  }
 }
}
'''
harness = harness.replace("/*SKILL*/", extract(skill, "public static double GetSkillChance(int"))
harness = harness.replace("/*METHOD*/", method).replace("/*MODIFIER*/", extract(recipe, "public static float GetMaterialMod("))
with tempfile.TemporaryDirectory(prefix="bace-craft-oracle-") as tmp:
    tmp = Path(tmp)
    (tmp / "Oracle.csproj").write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><EnableNETAnalyzers>false</EnableNETAnalyzers><NoWarn>0649</NoWarn></PropertyGroup></Project>')
    (tmp / "Program.cs").write_text(harness + material)
    subprocess.run([a.dotnet,"build",str(tmp / "Oracle.csproj"),"--nologo","-v:q"],check=True)
    output = subprocess.check_output([a.dotnet,str(tmp / "bin/Debug/net8.0/Oracle.dll")]).decode()
(root / "tests/fixtures/chance.csv").write_text("# Official ACE contributors; AGPL-3.0-only. Verbatim method, instrumented difficulty.\n" + rh + sh + mh + "# material,skill,tool,item,attempt,lum,difficulty,probability\n" + output)

player, ph = pinned(a.gdle,GDLE,"Source/Player.cpp")
method = extract(player,"int CPlayerWeenie::CalculateSalvageAmount(").replace("CPlayerWeenie::","")
code = '#include <cmath>\n#include <iostream>\nusing std::floor;\n' + method + r'''
int main(){for(int skill: {0,1,100,194,195,196,387,390,1000})for(int work: {1,2,5,10,99})for(int augs: {0,1,4})std::cout<<skill<<","<<work<<","<<augs<<","<<CalculateSalvageAmount(skill,work,augs)<<"\n";}
'''
with tempfile.TemporaryDirectory(prefix="bace-salvage-oracle-") as tmp:
    tmp = Path(tmp)
    (tmp / "oracle.cpp").write_text(code)
    subprocess.run(["c++","-std=c++17","-O0","-ffp-contract=off","-fno-fast-math",str(tmp / "oracle.cpp"),"-o",str(tmp / "oracle")],check=True)
    output = subprocess.check_output([str(tmp / "oracle")]).decode()
(root / "tests/fixtures/salvage.csv").write_text("# GDLE contributors, AGPL-3.0-only; verbatim compiled scalar method.\n"+ph+"# skill,workmanship,augmentations,units\n"+output)
