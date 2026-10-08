#!/usr/bin/env python3
"""Execute unchanged ACE inventory placement update statements on synthetic inputs."""
import argparse,hashlib,json,pathlib,subprocess,tempfile,urllib.request
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b';SOURCE='Source/ACE.Server/WorldObjects/Container.cs'
p=argparse.ArgumentParser();p.add_argument('--source',type=pathlib.Path,required=True);p.add_argument('--dotnet',default='dotnet');a=p.parse_args();data=(a.source/SOURCE).read_bytes();assert data==urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{SOURCE}',timeout=30).read();text=data.decode('utf-8-sig')
start=text.index('            // Move all the existing items PlacementPosition over.');end=text.index('            Inventory.Add(worldObject.Guid, worldObject);',start);add=text[start:end]
start=text.index('                // Move all the existing items PlacementPosition over.',end);end=text.index('\n',text.index('PlacementPosition--);',start))+1;remove=text[start:end]
program='''using System;using System.Linq;using System.Collections.Generic;
class Item {public int Id;public bool UseBackpackSlot;public int? PlacementPosition;}
class Harness {public Dictionary<int,Item> Inventory=new();
public void Add(Item worldObject,int placementPosition){var containerItems=Inventory.Values.ToList();worldObject.PlacementPosition=placementPosition;
'''+add+'''Inventory.Add(worldObject.Id,worldObject);}
public void Remove(Item item){int removedItemsPlacementPosition=item.PlacementPosition??0;Inventory.Remove(item.Id);
'''+remove+'''}
}
class Program {static Harness Fresh(){var h=new Harness();h.Inventory.Add(10,new Item{Id=10,PlacementPosition=0});h.Inventory.Add(11,new Item{Id=11,PlacementPosition=1});h.Inventory.Add(12,new Item{Id=12,PlacementPosition=0,UseBackpackSlot=true});return h;}
static void Main(){foreach(var pack in new[]{false,true})foreach(var slot in new[]{0,1}){var h=Fresh();h.Add(new Item{Id=13,UseBackpackSlot=pack},slot);Console.WriteLine($"add,{(pack?1:0)},{slot},"+string.Join(";",h.Inventory.Values.OrderBy(i=>i.Id).Select(i=>$"{i.Id}:{i.PlacementPosition}")));}foreach(var id in new[]{10,11,12}){var h=Fresh();h.Remove(h.Inventory[id]);Console.WriteLine($"remove,0,{id},"+string.Join(";",h.Inventory.Values.OrderBy(i=>i.Id).Select(i=>$"{i.Id}:{i.PlacementPosition}")));}}
}
'''
root=pathlib.Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='bace-inventory-oracle-') as temporary:
 b=pathlib.Path(temporary);(b/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>');(b/'Program.cs').write_text(program);subprocess.run([a.dotnet,'build','--nologo','-o',str(b/'out'),str(b/'Oracle.csproj')],check=True);result=subprocess.check_output([a.dotnet,str(b/'out/Oracle.dll')],text=True)
 (root/'tests/fixtures/placement.csv').write_text('# action,pack_slot,placement_or_removed_id,id:placement pairs\n'+result)
 (root/'tests/fixtures/placement.provenance.json').write_text(json.dumps({'repository':'https://github.com/ACEmulator/ACE','commit':PIN,'source':SOURCE,'source_sha256':hashlib.sha256(data).hexdigest(),'extracted_add_sha256':hashlib.sha256(add.encode()).hexdigest(),'extracted_remove_sha256':hashlib.sha256(remove.encode()).hexdigest(),'scope':'unchanged placement shift statements only; dictionary mutation/input values are harness adapters; no full handler/capacity claim'},indent=2)+'\n')
