#!/usr/bin/env python3
"""Compile the verbatim ACE GameEventPopupString event, its GameEventMessage base,
the GameMessage base and the Extensions.WriteString16L writer, then emit packets.

Only the session/player/enum types are stubbed; they are reduced to the fields the
verbatim code touches. Every extracted source is hashed into the fixture header.
"""
from pathlib import Path
import hashlib, os, re, subprocess, tempfile

ROOT = Path(__file__).resolve().parents[4]
NETWORK = ROOT / '.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Server/Network'
DOTNET = os.environ.get('BACE_DOTNET', '/home/jokerfactor/.dotnet/dotnet')
FIXTURE = ROOT / 'crates/network/bace-wire/tests/fixtures/popup.csv'


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


popup_src = (NETWORK / 'GameEvent/Events/GameEventPopupString.cs').read_text()
base_src = (NETWORK / 'GameEvent/GameEventMessage.cs').read_text()
message_src = (NETWORK / 'GameMessages/GameMessage.cs').read_text()
extensions_src = (NETWORK / 'Extensions.cs').read_text()
event_enum_src = (NETWORK / 'GameEvent/GameEventType.cs').read_text()

popup_class = method(popup_src, 'public class GameEventPopupString')
base_class = method(base_src, 'public abstract class GameEventMessage')
message_class = method(message_src, 'public abstract class GameMessage')
extension_methods = [
    method(extensions_src, 'private static uint CalculatePadMultiple(uint length, uint multiple)'),
    method(extensions_src, 'public static void WriteString16L(this BinaryWriter writer, string data)'),
    method(extensions_src, 'public static void Pad(this BinaryWriter writer, uint pad)'),
    method(extensions_src, 'public static void WriteGuid(this BinaryWriter writer, ObjectGuid guid)'),
]
popup_value = re.search(r'\bPopupString\s*=\s*(0x[\dA-Fa-f]+)', event_enum_src).group(1)

# Cases: name, UTF-8 hex of one unit, repeat count. The C# harness builds the same text.
CASES = [
    ('empty', '', 0),
    ('ascii', '506F7075703A20414345', 1),          # "Popup: ACE"
    ('cp1252_e_acute', 'C3A9', 1),                  # U+00E9 -> 0xE9
    ('max_length', '41', 65535),                    # 65535 bytes, the u16 length limit
    ('unmappable_substitution', 'E4B8AD', 1),       # U+4E2D is not in Windows-1252
]

cases_code = '\n'.join(
    f'Emit("{name}", "{unit}", {repeat});' for name, unit, repeat in CASES
)

code = '''using System;
using System.IO;
using System.Linq;
using System.Text;

public struct ObjectGuid { public uint Full; public ObjectGuid(uint value) { Full = value; } }
public class Player { public ObjectGuid Guid; }
public class Session { public Player Player; public uint GameEventSequence; }
public enum GameMessageGroup { UIQueue = 0x09 }
public enum GameMessageOpcode { None = 0x0000, GameEvent = 0xF7B0 }
public enum GameEventType { PopupString = ''' + popup_value + ''' }

public static class Extensions
{
''' + '\n'.join(extension_methods) + '''
}

''' + message_class + '''

''' + base_class + '''

''' + popup_class + '''

class Program
{
    static void Emit(string name, string unitHex, int repeat)
    {
        var unit = Encoding.UTF8.GetString(Convert.FromHexString(unitHex));
        var text = string.Concat(Enumerable.Repeat(unit, repeat));
        var session = new Session { Player = new Player { Guid = new ObjectGuid(0x50000001) }, GameEventSequence = 42 };
        var message = new GameEventPopupString(session, text);
        Console.WriteLine("case," + name + "," + unitHex + "," + repeat + "," + Convert.ToHexString(message.Data.ToArray()));
    }

    static void Main()
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
''' + cases_code + '''
    }
}
'''

with tempfile.TemporaryDirectory(prefix='bace-popup-oracle-') as directory:
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
    'GameEventPopupString': popup_src,
    'GameEventMessage': base_src,
    'GameMessage': message_src,
    'Extensions': extensions_src,
    'GameEventType': event_enum_src,
}
header = ['# ACE47edade3bd3f6044b676d4eb877c4965c7eda62b',
          '# GameEvent PopupString 0x0004; object_id 0x50000001 sequence 42 (session counter)']
header += [f'# {name} {hashlib.sha256(text.encode()).hexdigest()}' for name, text in provenance.items()]
header += ['# columns: name,unit_utf8_hex,repeat,packet_hex,packet_len,head64_hex,tail64_hex',
           '# packets longer than 1024 bytes are summarised: packet_hex is empty and the',
           '# exact length plus the first and last 64 ACE bytes are recorded instead.']
SUMMARY_THRESHOLD = 1024
lines = []
for row in rows:
    _, name, unit, repeat, packet_hex = row.split(',')
    packet = bytes.fromhex(packet_hex)
    if len(packet) > SUMMARY_THRESHOLD:
        lines.append(f'{name},{unit},{repeat},,{len(packet)},{packet[:64].hex().upper()},{packet[-64:].hex().upper()}')
    else:
        lines.append(f'{name},{unit},{repeat},{packet_hex},,,')
FIXTURE.write_text('\n'.join(header + lines) + '\n')
print(FIXTURE, len(rows), 'original popup packets')
