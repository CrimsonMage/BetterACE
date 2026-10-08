#!/usr/bin/env python3
"""Original ACE GetDefaultMotion/GetAnimData/GetAnimationLength timing vectors."""
from pathlib import Path
import hashlib, subprocess, tempfile
ROOT=Path(__file__).resolve().parents[4]
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
source=ROOT/'.reference'/('ACE-'+PIN)/'Source/ACE.DatLoader/FileTypes/MotionTable.cs'
text=source.read_text()
def method(signature):
    a=text.index(signature); i=text.index('{',a)+1; depth=1
    while depth: depth+=(text[i]=='{')-(text[i]=='}'); i+=1
    return text[a:i]
methods='\n'.join(method(s) for s in ['private MotionCommand GetDefaultMotion(', 'public float GetAnimationLength(MotionCommand motion)', 'public List<AnimData> GetAnimData(', 'public float GetAnimationLength(MotionStance stance, MotionCommand motion, MotionCommand currentMotion)', 'public float GetAnimationLength(AnimData anim)'])
program=r'''using System;using System.Collections.Generic;
enum MotionStance:uint { Peace=0x8000003d } enum MotionCommand:uint { Invalid=0, Ready=0x41000003, Recall=0x10000153 }
class AnimData{public uint AnimId;public int LowFrame,HighFrame;public float Framerate;}
class Animation{public uint NumFrames;}
class Archive{public T ReadFromDat<T>(uint id) where T:class => new Animation{NumFrames=id==1?37u:53u} as T;}
class DatManager{public static Archive PortalDat=new();}
class MotionData{public List<AnimData>Anims=new();}
class MotionTable{public uint DefaultStyle=0x8000003d;public Dictionary<uint,uint>StyleDefaults=new();public Dictionary<uint,Dictionary<uint,MotionData>>Links=new();
METHODS
}
class Program{static void Main(){for(int layout=0;layout<4;layout++)foreach(int low in new[]{0,2,13})foreach(int high in new[]{-1,20,37,90})foreach(float rate in new[]{7f,30f,-24f})foreach(int count in new[]{1,3}){var t=new MotionTable();t.StyleDefaults[t.DefaultStyle]=(uint)MotionCommand.Ready;uint key=t.DefaultStyle<<16|((uint)MotionCommand.Ready&0xFFFFF);if(layout!=0)t.Links[key]=new();if(layout==2)t.Links[t.DefaultStyle<<16]=new();var data=new MotionData();for(int i=0;i<count;i++)data.Anims.Add(new AnimData{AnimId=(uint)(i%2+1),LowFrame=low,HighFrame=high,Framerate=rate});if(layout>=2)t.Links[layout==2?t.DefaultStyle<<16:key][(uint)MotionCommand.Recall]=data;Console.WriteLine($"{layout},{low},{high},{rate},{count},{BitConverter.SingleToUInt32Bits(t.GetAnimationLength(MotionCommand.Recall))}");}}}
'''.replace('METHODS',methods)
with tempfile.TemporaryDirectory(prefix='bace-recall-timing-') as tmp:
    d=Path(tmp);(d/'Program.cs').write_text(program);(d/'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><WarningLevel>0</WarningLevel></PropertyGroup></Project>')
    dotnet='/tmp/bace-crafting-dotnet/dotnet'
    subprocess.run([dotnet,'build','--nologo','-o',str(d/'build')],cwd=d,check=True)
    output=subprocess.check_output([dotnet,str(d/'build/oracle.dll')],text=True)
    (Path(__file__).resolve().parents[1]/'tests/fixtures/recall_timing.csv').write_text('# official ACE '+PIN+'\n# ACE.DatLoader/FileTypes/MotionTable.cs sha256='+hashlib.sha256(source.read_bytes()).hexdigest()+'\n# layout,low,high,rate,count,seconds_bits\n'+output)
    print('original-source recall timing vectors:',len(output.splitlines()))
