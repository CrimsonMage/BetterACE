#!/usr/bin/env python3
"""Execute unchanged pinned split-to-container completion for output ordering.
Only world/network boundaries are stubbed; gameplay method body is unmodified.
"""
from pathlib import Path
import hashlib,json,subprocess,tempfile
root=Path(__file__).resolve().parents[5]
pin=root/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
path=pin/'ACE.Server/WorldObjects/Player_Inventory.cs';source=path.read_text()
sig='private bool DoHandleActionStackableSplitToContainer('
a=source.index(sig);b=source.index('{',a)+1;depth=1
while depth:depth+=(source[b]=='{')-(source[b]=='}');b+=1
method=source[a:b]
code='''using System; using System.Collections.Generic; using System.Text.Json;
public record GuidValue(uint Full);
public class WorldObject{public GuidValue Guid=new(1);public int StackUnitEncumbrance=2,StackUnitValue=3,EncumbranceVal,Value;}
public class Container:WorldObject{public bool Accepted=true;public bool TryAddToInventory(WorldObject item,int placement,bool change)=>Accepted;}
public class Network{public List<string> Messages=new();public void EnqueueSend(object message)=>Messages.Add(message.ToString());}
public class SessionValue{public Network Network=new();}
public class GameMessageCreateObject{public GameMessageCreateObject(WorldObject w){}public override string ToString()=>"create";}
public class GameEventItemServerSaysContainId{public GameEventItemServerSaysContainId(SessionValue s,WorldObject w,WorldObject c){}public override string ToString()=>"contain";}
public class GameMessageSetStackSize{public GameMessageSetStackSize(WorldObject w){}public override string ToString()=>"stack";}
public class GameEventCommunicationTransientString{public GameEventCommunicationTransientString(SessionValue s,string text){}public override string ToString()=>"error-text";}
public class GameEventInventoryServerSaveFailed{public GameEventInventoryServerSaveFailed(SessionValue s,uint id){}public override string ToString()=>"save-failed";}
public class Player:Container{public SessionValue Session=new();bool AdjustStack(WorldObject stack,int amount,Container found,Container root)=>true;void EnqueueBroadcast(object m)=>Session.Network.EnqueueSend(m);
'''+method+'''
public object Run(bool accepted){var target=new Container{Accepted=accepted};var result=DoHandleActionStackableSplitToContainer(new WorldObject(),this,this,target,this,new WorldObject(),0,2);return new{accepted,result,messages=Session.Network.Messages};}}
class Program{static void Main(){Console.WriteLine(JsonSerializer.Serialize(new[]{new Player().Run(true),new Player().Run(false)}));}}
'''
with tempfile.TemporaryDirectory(prefix='bace-split-order-')as tmp:
 p=Path(tmp);(p/'Program.cs').write_text(code);(p/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
 dotnet='/tmp/bace-crafting-dotnet/dotnet'
 result=subprocess.run([dotnet,'build','--nologo','-o',str(p/'out')],cwd=p,capture_output=True,text=True)
 if result.returncode:print(result.stdout);result.check_returncode()
 rows=json.loads(subprocess.run([dotnet,str(p/'out/oracle.dll')],capture_output=True,text=True,check=True).stdout)
 target=root/'crates/application/bace-runtime/tests/fixtures/inventory_split_order.json';target.write_text(json.dumps({'source_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'cases':rows},indent=2)+'\n');print(target)
