#!/usr/bin/env python3
"""Compile unchanged ACE mana heartbeat/low-warning/depletion methods."""
from pathlib import Path
import tempfile,subprocess,os,hashlib
ROOT=Path(__file__).resolve().parents[4]
source=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Server/WorldObjects/Player_Tick.cs'
text=source.read_text()
def method(signature,text=text):
    start=text.index(signature);begin=text.index('{',start);end=begin+1;depth=1
    while depth:
        if text[end]=='{':depth+=1
        elif text[end]=='}':depth-=1
        end+=1
    return text[start:end]
rating_source=source.with_name("Creature_Rating.cs");rating_text=rating_source.read_text()
code='''using System;using System.Collections.Generic;using System.Globalization;
enum ChatMessageType{Magic} enum Sound{ItemManaDepleted}
class GameMessageSystemChat{public string Text;public GameMessageSystemChat(string s,ChatMessageType t){Text=s;}}
class GameMessageSound{public GameMessageSound(uint id,Sound sound){}}
class Network{public int Warnings,Depleted,Sounds;public void EnqueueSend(params object[] messages){foreach(var m in messages)if(m is GameMessageSystemChat c){if(c.Text.Contains("low"))Warnings++;else Depleted++;}else Sounds++;}}
class Session{public Network Network=new();}
class Biota{public List<int> GetKnownSpellsIds(object x)=>new(){1,2,3};}
class WorldObject{public string Name="Test";public int? ItemCurMana,ItemMaxMana=100;public double? ManaRate;public bool IsAffecting=true;public float ItemManaRateAccumulator;public bool ItemManaDepletionMessage;public Biota Biota=new();public object BiotaDatabaseLock=new();public void OnSpellsDeactivated(){IsAffecting=false;}}
class ActionChain{public static float Delay;public void AddDelaySeconds(float time){Delay=time;}public void AddAction(Player p,Action action){}public void EnqueueChain(){}}
class Player{public bool EquippedObjectsLoaded=true;public Dictionary<int,WorldObject> EquippedObjects=new();public int LumAugItemManaUsage;public double CachedHeartbeatInterval;public uint Guid=1;public Session Session=new();public void RemoveItemSpell(WorldObject item,uint id){}
'''+method('        public static float GetPositiveRatingMod(',rating_text)+method('        public static float GetNegativeRatingMod(',rating_text)+method('        public void ManaConsumersTick(')+method('        private bool CheckLowMana(')+method('        private void HandleManaDepleted(')+'''
public static void Main(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;foreach(int mana in new[]{0,1,2,20,100})foreach(double rate in new[]{-0.01,-0.2,-2.0,0.1})foreach(double interval in new[]{1.0,5.0,7.5})foreach(int rating in new[]{0,1,20})foreach(bool warned in new[]{false,true})foreach(float accumulator in new[]{0f,0.75f,1.5f}){var item=new WorldObject{ItemCurMana=mana,ManaRate=rate,ItemManaDepletionMessage=warned,ItemManaRateAccumulator=accumulator};var p=new Player{LumAugItemManaUsage=rating,CachedHeartbeatInterval=interval};p.EquippedObjects[1]=item;ActionChain.Delay=0;p.ManaConsumersTick();var n=p.Session.Network;Console.WriteLine($"{mana}|{rate:R}|{interval:R}|{rating*5}|{warned}|{BitConverter.SingleToUInt32Bits(accumulator)}|{item.ItemCurMana}|{BitConverter.SingleToUInt32Bits(item.ItemManaRateAccumulator)}|{item.ItemManaDepletionMessage}|{item.IsAffecting}|{n.Warnings}|{n.Depleted}|{n.Sounds}|{ActionChain.Delay}");}}
}'''
with tempfile.TemporaryDirectory(prefix='bace-mana-oracle-') as d:
    p=Path(d);(p/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>');(p/'Program.cs').write_text(code)
    run=subprocess.run([os.environ.get('DOTNET','/tmp/bace-crafting-dotnet/dotnet'),'run','--project',str(p/'Oracle.csproj'),'-c','Release'],text=True,capture_output=True)
    if run.returncode:raise RuntimeError(run.stdout+run.stderr)
    rows=[line for line in run.stdout.splitlines() if '|' in line]
    target=ROOT/'crates/simulation/bace-simulation/tests/fixtures/equipment_mana.txt'
    target.write_text('# ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b Player_Tick.cs sha256 '+hashlib.sha256(text.encode()).hexdigest()+'\n# Creature_Rating.cs sha256 '+hashlib.sha256(rating_text.encode()).hexdigest()+'\n'+'\n'.join(rows)+'\n')
    print(len(rows),'original ACE mana vectors')
