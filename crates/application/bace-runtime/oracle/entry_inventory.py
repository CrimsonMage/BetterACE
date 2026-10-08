#!/usr/bin/env python3
"""Compile unchanged ACE PlayerDescription inventory serialization statements."""
from pathlib import Path
import hashlib
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[4]
SOURCE = ROOT / '.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
player = SOURCE / 'ACE.Server/Network/GameEvent/Events/GameEventPlayerDescription.cs'
enums = [SOURCE / 'ACE.Entity/Enum' / (name + '.cs') for name in ['WeenieType', 'ContainerType']]
source = player.read_text()
start = source.index('            Writer.Write((uint)Session.Player.Inventory.Count);')
end = source.index('            Writer.Write((uint)Session.Player.EquippedObjects.Values.Count);', start)
body = source[start:end]
with tempfile.TemporaryDirectory(prefix='bace-entry-inventory-') as directory:
    path = Path(directory)
    for enum in enums:
        (path / enum.name).write_text(enum.read_text())
    (path / 'Program.cs').write_text('''
using System; using System.IO; using System.Linq; using System.Collections.Generic; using ACE.Entity.Enum;
class GuidStub { public uint Full; }
class Item { public GuidStub Guid = new(); public WeenieType WeenieType; public bool UseBackpackSlot; public int PlacementPosition; }
class PlayerStub { public Dictionary<uint,Item> Inventory = new(); }
class SessionStub { public PlayerStub Player = new(); }
class Program {
    BinaryWriter Writer; SessionStub Session = new();
    void Serialize() {''' + body + '''}
    static void Main() {
        var p = new Program(); using var stream = new MemoryStream(); p.Writer = new BinaryWriter(stream);
        uint[] types = {21,1,1,21}; bool[] packs = {true,true,false,false};
        for (int i=0;i<4;i++) { var item = new Item { WeenieType=(WeenieType)types[i],UseBackpackSlot=packs[i],PlacementPosition=i };
            item.Guid.Full=0x80000001u+(uint)i; p.Session.Player.Inventory.Add(item.Guid.Full,item); }
        p.Serialize(); Console.WriteLine(Convert.ToHexString(stream.ToArray()).ToLowerInvariant());
    }
}''')
    (path / 'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
    dotnet = '/tmp/bace-crafting-dotnet/dotnet'
    subprocess.run([dotnet, 'build', '--nologo', '-o', str(path / 'build')], cwd=path, check=True)
    output = subprocess.check_output([dotnet, str(path / 'build/oracle.dll')], text=True)
target = Path(__file__).resolve().parents[1] / 'tests/fixtures/entry_inventory.csv'
target.write_text(''.join('# ' + str(file.relative_to(SOURCE)) + ' ' + hashlib.sha256(file.read_bytes()).hexdigest() + '\n' for file in [player, *enums]) + output)
