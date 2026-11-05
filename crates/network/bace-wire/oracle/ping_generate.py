#!/usr/bin/env python3
"""Compile the verbatim ACE GameEventPingResponse event and its GameEventMessage base
(4-byte capacity constructor), then emit the packets. Only session, player and enum
types are stubbed to the fields the verbatim code touches.
"""
from pathlib import Path
import hashlib, os, re, subprocess, tempfile

ROOT = Path(__file__).resolve().parents[4]
NETWORK = ROOT / '.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Server/Network'
DOTNET = os.environ.get('BACE_DOTNET', '/home/jokerfactor/.dotnet/dotnet')
FIXTURE = ROOT / 'crates/network/bace-wire/tests/fixtures/ping.csv'


def method(text, marker):
    """Return the verbatim source from marker through its matching closing brace."""
    start = text.index(marker)
    end = text.index('{', start) + 1
    depth = 1
    while depth:
        if text[end] == '{':
            depth += 1
        elif text[end] == '}':
            depth -= 1
        end += 1
    return text[start:end]


ping_src = (NETWORK / 'GameEvent/Events/GameEventPingResponse.cs').read_text()
base_src = (NETWORK / 'GameEvent/GameEventMessage.cs').read_text()
message_src = (NETWORK / 'GameMessages/GameMessage.cs').read_text()
extensions_src = (NETWORK / 'Extensions.cs').read_text()
event_enum_src = (NETWORK / 'GameEvent/GameEventType.cs').read_text()

ping_class = method(ping_src, 'public class GameEventPingResponse')
base_class = method(base_src, 'public abstract class GameEventMessage')
message_class = method(message_src, 'public abstract class GameMessage')
extension_methods = [
    method(extensions_src, 'public static void WriteGuid(this BinaryWriter writer, ObjectGuid guid)'),
]
ping_value = re.search(r'\bPingResponse\s*=\s*(0x[\dA-Fa-f]+)', event_enum_src).group(1)

# name, object_id (None = no attached player, so ACE writes guid 0), sequence
CASES = [
    ('entered', 0x50000001, 42),
    ('no_player', None, 7),
]

cases_code = '\n'.join(
    f'Emit("{name}", {("null" if guid is None else f"new ObjectGuid(0x{guid:08X})")}, {sequence});'
    for name, guid, sequence in CASES
)

code = '''using System;
using System.IO;
using System.Text;

public struct ObjectGuid { public uint Full; public ObjectGuid(uint value) { Full = value; } }
public class Player { public ObjectGuid Guid; }
public class Session { public Player Player; public uint GameEventSequence; }
public enum GameMessageGroup { UIQueue = 0x09 }
public enum GameMessageOpcode { None = 0x0000, GameEvent = 0xF7B0 }
public enum GameEventType { PingResponse = ''' + ping_value + ''' }

public static class Extensions
{
''' + '\n'.join(extension_methods) + '''
}

''' + message_class + '''

''' + base_class + '''

''' + ping_class + '''

class Program
{
    static void Emit(string name, ObjectGuid? guid, uint sequence)
    {
        var session = new Session { Player = guid.HasValue ? new Player { Guid = guid.Value } : null, GameEventSequence = sequence };
        var message = new GameEventPingResponse(session);
        Console.WriteLine("case," + name + "," + (guid.HasValue ? guid.Value.Full.ToString("X8") : "00000000") + "," + sequence + "," + Convert.ToHexString(message.Data.ToArray()));
    }

    static void Main()
    {
''' + cases_code + '''
    }
}
'''

with tempfile.TemporaryDirectory(prefix='bace-ping-oracle-') as directory:
    p = Path(directory)
    (p / 'Oracle.csproj').write_text(
        '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType>'
        '<TargetFramework>net8.0</TargetFramework><Nullable>disable</Nullable>'
        '<ImplicitUsings>disable</ImplicitUsings></PropertyGroup></Project>'
    )
    (p / 'Program.cs').write_text(code)
    result = subprocess.check_output(
        [DOTNET, 'run', '--project', str(p / 'Oracle.csproj'), '-c', 'Release'], text=True
    )

rows = [line for line in result.splitlines() if line.startswith('case,')]
assert len(rows) == len(CASES), rows
provenance = {
    'GameEventPingResponse': ping_src,
    'GameEventMessage': base_src,
    'GameMessage': message_src,
    'Extensions.WriteGuid': extensions_src,
    'GameEventType': event_enum_src,
}
header = ['# ACE47edade3bd3f6044b676d4eb877c4965c7eda62b',
          '# GameEvent PingResponse 0x01EA; guid from the attached player (0 when none), session GameEventSequence']
header += [f'# {name} {hashlib.sha256(text.encode()).hexdigest()}' for name, text in provenance.items()]
header += ['# columns: name,object_id_hex,sequence,packet_hex']
FIXTURE.write_text('\n'.join(header + [','.join(row.split(',')[1:]) for row in rows]) + '\n')
print(FIXTURE, len(rows), 'original ping response packets')
