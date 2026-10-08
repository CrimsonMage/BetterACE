#!/usr/bin/env python3
"""Pinned ACE rent-boundary oracle; pure clock and house properties are adapters."""
import argparse,hashlib,json,pathlib,subprocess,tempfile,urllib.request
PIN='47edade3bd3f6044b676d4eb877c4965c7eda62b'
SOURCE='Source/ACE.Server/WorldObjects/House.cs'
p=argparse.ArgumentParser();p.add_argument('--source',type=pathlib.Path,required=True);p.add_argument('--dotnet',default='dotnet');a=p.parse_args()
data=(a.source/SOURCE).read_bytes();official=urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{SOURCE}',timeout=30).read();assert data==official
text=data.decode('utf-8-sig')
def method(signature):
 start=text.index(signature);brace=text.index('{',start);level=1;end=brace+1
 while level:
  level+=(text[end]=='{')-(text[end]=='}');end+=1
 return text[start:end]
methods='\n'.join(method(s) for s in ['public uint GetRentTimestamp(uint purchaseTime)','public uint GetRentDue(uint purchaseTime)'])
program='''using System;
public static class Time {public static double Now;public static double GetUnixTime()=>Now;}
public class House {public bool IsApartment;public TimeSpan RentInterval=TimeSpan.FromDays(30);
'''+methods+'''
}
class Program {static void Main(){foreach(var apartment in new[]{false,true})foreach(uint purchase in new uint[]{0,100,1700000000})foreach(uint delta in new uint[]{0,1,2591999,2592000,2592001,7776000,31536000}){Time.Now=purchase+delta;var h=new House{IsApartment=apartment};Console.WriteLine($"{(apartment?1:0)},{purchase},{purchase+delta},{h.GetRentTimestamp(purchase)},{h.GetRentDue(purchase)}");}}}
'''
root=pathlib.Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='bace-rent-oracle-') as temporary:
 build=pathlib.Path(temporary);(build/'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>');(build/'Program.cs').write_text(program)
 subprocess.run([a.dotnet,'build','--nologo','-o',str(build/'out'),str(build/'Oracle.csproj')],check=True)
 result=subprocess.check_output([a.dotnet,str(build/'out/Oracle.dll')],text=True)
 (root/'tests/fixtures/rent.csv').write_text('# apartment,purchase,now,period,due\n'+result)
 (root/'tests/fixtures/rent.provenance.json').write_text(json.dumps({'repository':'https://github.com/ACEmulator/ACE','commit':PIN,'source':SOURCE,'source_sha256':hashlib.sha256(data).hexdigest(),'extracted_methods_sha256':hashlib.sha256(methods.encode()).hexdigest(),'scope':'unchanged GetRentTimestamp/GetRentDue; explicit synthetic clock and 30-day rent interval'},indent=2)+'\n')
