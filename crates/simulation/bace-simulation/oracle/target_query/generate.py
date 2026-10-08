#!/usr/bin/env python3
"""Run pinned Player query methods and WorldObject query arithmetic unchanged."""
from pathlib import Path
import hashlib,os,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[5]
base=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Server'
player=(base/'WorldObjects/Player.cs').read_text();world=(base/'WorldObjects/WorldObject.cs').read_text();creature=(base/'WorldObjects/Creature.cs').read_text()
def method(text,marker):
 s=text.index(marker);e=text.index('{',s)+1;d=1
 while d:
  if text[e]=='{':d+=1
  elif text[e]=='}':d-=1
  e+=1
 return text[s:e]
code='''using System;using System.Collections.Generic;
class GuidValue {public uint Full; public GuidValue(uint value){Full=value;}}
class Session {public Network Network=new();}class Network {public List<string> Events=new();public void EnqueueSend(object e){Events.Add(e.ToString());}}
class GameEventUpdateHealth {uint Target;float Value;public GameEventUpdateHealth(Session s,uint target,float value){Target=target;Value=value;}public override string ToString()=>$"H:{Target}:{BitConverter.SingleToInt32Bits(Value):X8}";}
class GameEventQueryItemManaResponse {uint Target,Success;float Value;public GameEventQueryItemManaResponse(Session s,uint target,float value,uint success){Target=target;Value=value;Success=success;}public override string ToString()=>$"M:{Target}:{BitConverter.SingleToInt32Bits(Value):X8}:{Success}";}
class WorldObject {public GuidValue Guid;public int? ItemCurMana,ItemMaxMana;
'''+method(world,'        public void QueryHealth(')+method(world,'        public void QueryItemMana(')+'''}
class Pool {public uint Current,MaxValue;}
class Creature:WorldObject {public Pool Health=new();public int Selected,Deselected;Dictionary<uint,WorldObjectInfo> selectedTargets=new();
'''+method(creature,'        public bool OnTargetSelected(').replace('OnTargetSelected(', 'SourceOnTargetSelected(')+method(creature,'        public bool OnTargetDeselected(').replace('OnTargetDeselected(', 'SourceOnTargetDeselected(')+method(creature,'        public void OnHealthUpdate(')+'''
public void OnTargetSelected(Player p){if(SourceOnTargetSelected(p))Selected++;}public void OnTargetDeselected(Player p){if(SourceOnTargetDeselected(p))Deselected++;}}

class WorldObjectInfo {WorldObject Target;public WorldObjectInfo(WorldObject target){Target=target;}public WorldObject TryGetWorldObject()=>Target;}
class Landblock {public Dictionary<uint,WorldObject> Objects=new();public WorldObject GetObject(uint id)=>Objects.TryGetValue(id,out var value)?value:null;}
class Player:Creature {public Session Session=new();public Landblock CurrentLandblock=new();public uint? HealthQueryTarget,ManaQueryTarget;WorldObjectInfo selectedTarget;public Dictionary<uint,WorldObject> Inventory=new();WorldObject GetInventoryItem(uint id)=>Inventory.TryGetValue(id,out var value)?value:null;WorldObject GetEquippedItem(uint id)=>null;
'''+method(player,'        public void HandleActionQueryHealth(')+method(player,'        private void UpdateSelectedTarget(')+method(player,'        public void HandleActionQueryItemMana(')+'''
static string Optional(uint? value)=>value?.ToString()??"-";
static void Main(){foreach(int current in new[]{0,5,10})foreach(int maximum in new[]{0,10})foreach(bool hasMana in new[]{false,true}){var p=new Player{Guid=new(1)};var creature=new Creature{Guid=new(2),Health=new(){Current=(uint)current,MaxValue=(uint)maximum}};var item=new WorldObject{Guid=new(3),ItemCurMana=hasMana?current:null,ItemMaxMana=hasMana?maximum:null};p.CurrentLandblock.Objects.Add(2,creature);p.CurrentLandblock.Objects.Add(3,item);p.Inventory.Add(3,item);foreach(var step in new[]{(0,2u),(0,3u),(0,2u),(0,0u),(1,3u),(1,999u),(1,0u)}){p.Session.Network.Events.Clear();if(step.Item1==0)p.HandleActionQueryHealth(step.Item2);else p.HandleActionQueryItemMana(step.Item2);Console.WriteLine($"{current}|{maximum}|{hasMana}|{step.Item1}|{step.Item2}|{Optional(p.HealthQueryTarget)}|{Optional(p.ManaQueryTarget)}|{string.Join(',',p.Session.Network.Events)}|{creature.Selected}|{creature.Deselected}");}}var observer=new Player{Guid=new(1)};var watched=new Creature{Guid=new(2),Health=new(){Current=10,MaxValue=10}};watched.OnTargetSelected(observer);watched.Health.Current=8;watched.OnHealthUpdate();watched.Health.Current=5;watched.OnHealthUpdate();watched.OnTargetDeselected(observer);watched.Health.Current=4;watched.OnHealthUpdate();foreach(var update in observer.Session.Network.Events)Console.WriteLine($"U|{update}");}}

'''
with tempfile.TemporaryDirectory(prefix='bace-target-query-') as d:
 p=Path(d);(p/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>');(p/'Program.cs').write_text(code)
 out=subprocess.check_output([os.environ.get('DOTNET','/tmp/bace-crafting-dotnet/dotnet'),'run','--project',str(p/'Oracle.csproj'),'-c','Release'],text=True)
 rows=[l for l in out.splitlines() if l.startswith(('0|','5|','10|'))];assert len(rows)==84
 (ROOT/'crates/simulation/bace-simulation/tests/fixtures/target_query.txt').write_text('# ACE47edade3bd3f6044b676d4eb877c4965c7eda62b Player.cs '+hashlib.sha256(player.encode()).hexdigest()+' WorldObject.cs '+hashlib.sha256(world.encode()).hexdigest()+'\n'+'\n'.join(rows)+'\n');print(len(rows),'original target query vectors')
 updates=[l for l in out.splitlines() if l.startswith('U|')];assert len(updates)==2
 (ROOT/'crates/simulation/bace-simulation/tests/fixtures/health_updates.txt').write_text('# Original Creature.OnTargetSelected/OnTargetDeselected/OnHealthUpdate '+hashlib.sha256(creature.encode()).hexdigest()+'\n'+'\n'.join(updates)+'\n')
