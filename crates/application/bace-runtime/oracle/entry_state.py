#!/usr/bin/env python3
"""Compile original ACE initial physics calculation and its actual property wrappers."""
from pathlib import Path
import hashlib,re,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[4]
SRC=ROOT/'.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
net=SRC/'ACE.Server/WorldObjects/WorldObject_Networking.cs'
props=SRC/'ACE.Server/WorldObjects/WorldObject_Properties.cs'
player=SRC/'ACE.Server/WorldObjects/Player.cs'
enum=SRC/'ACE.Entity/Enum/PhysicsState.cs'
boolenum=SRC/'ACE.Entity/Enum/Properties/PropertyBool.cs'
def method(text,signature):
 a=text.index(signature); i=text.index('{',a)+1; depth=1
 while depth: depth+=(text[i]=='{')-(text[i]=='}'); i+=1
 return text[a:i]
p=props.read_text(); wrappers=p[p.index('        public bool? Static'):p.index('        // ========================================',p.index('        public bool? IsFrozen'))]
methods='\n'.join(method(p,s) for s in ['public bool GetPhysicsState(', 'public void SetPhysicsState(', 'public void SetPhysicsPropertyState('])
methods+='\n'+method(net.read_text(),'private PhysicsState GetPhysicsStateOrDefault(')+'\n'+method(net.read_text(),'private PhysicsState CalculatedPhysicsState(')
names=re.findall(r'PropertyBool\.(\w+)',wrappers); names=list(dict.fromkeys(names))
ids={n:int(re.search(r'\b'+n+r'\s*=\s*(\d+)',boolenum.read_text()).group(1)) for n in names}
pink=player.read_text(); pink=pink[pink.index('            IgnoreCollisions = true;'):pink.index('\n',pink.index('            IgnoreCollisions = true;'))]
s='''using System;using System.Collections.Generic;using ACE.Entity.Enum;
enum PropertyInt{PhysicsState} enum PropertyBool{ENUM}
class PhysicsGlobals{public const PhysicsState DefaultState=PhysicsState.EdgeSlide|PhysicsState.LightingOn|PhysicsState.Gravity|PhysicsState.ReportCollisions;}
class Obj{public PhysicsState State;public bool HasDefaultAnimation,HasDefaultScript;}
class Setup{public bool HasPhysicsBSP;public uint DefaultAnimation,DefaultScript;}
class WorldObject{public Obj PhysicsObj;public Setup CSetup=new(); public int? saved;public Dictionary<PropertyBool,bool> values=new();
public int? GetProperty(PropertyInt p)=>saved;public bool? GetProperty(PropertyBool p)=>values.TryGetValue(p,out var v)?v:null;
public void SetProperty(PropertyBool p,bool v)=>values[p]=v;public void RemoveProperty(PropertyBool p)=>values.Remove(p);
WRAPPERS
METHODS
public uint Run(){var initial=CalculatedPhysicsState();PhysicsObj=new Obj{State=initial};if(this is Player){PINK}return (uint)PhysicsObj.State;}}
class Player:WorldObject{}
class Program{static void Main(){foreach(var player in new[]{false,true})foreach(int? saved in new int?[]{null,0,-1,0x404410})foreach(var bsp in new[]{false,true})foreach(var key in new[]{KEYS})foreach(var value in new[]{-1,0,1}){WorldObject w=player?new Player():new WorldObject();w.saved=saved;w.CSetup.HasPhysicsBSP=bsp;if(value>=0)w.values[(PropertyBool)key]=value!=0;Console.WriteLine($"{(player?1:0)},{(saved.HasValue?saved.Value.ToString():"none")},{(bsp?8:0)},{key},{value},{w.Run()}");}}}
'''.replace('ENUM',','.join(f'{n}={v}' for n,v in ids.items())).replace('WRAPPERS',wrappers).replace('METHODS',methods).replace('PINK',pink).replace('KEYS',','.join(map(str,ids.values())))
with tempfile.TemporaryDirectory(prefix='entry-state-') as tmp:
 d=Path(tmp);(d/'Program.cs').write_text(s);(d/'PhysicsState.cs').write_bytes(enum.read_bytes());(d/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><WarningLevel>0</WarningLevel></PropertyGroup></Project>')
 subprocess.run(['/tmp/bace-crafting-dotnet/dotnet','build','--nologo','-o',str(d/'build')],cwd=d,check=True)
 output=subprocess.check_output(['/tmp/bace-crafting-dotnet/dotnet',str(d/'build/oracle.dll')],text=True)
 dest=Path(__file__).parents[1]/'tests/fixtures/entry_state.csv'
 dest.write_text(''.join('# '+str(f.relative_to(SRC))+' '+hashlib.sha256(f.read_bytes()).hexdigest()+'\n' for f in [net,props,player,enum,boolenum])+output)
 print('rows',len(output.splitlines()))
