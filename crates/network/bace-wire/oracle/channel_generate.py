#!/usr/bin/env python3
"""Compile the verbatim ACE channel-action read statements and emit a deterministic fixture.

Sources: ACE GameActionChannelList.cs (ListChannels 0x0148, one uint32 read) and
GameActionChannelIndex.cs (IndexChannels 0x0149, no reads). GameActionEnterPkLite.cs (0x028F)
is asserted to read no payload. The verbatim read statements are compiled in a C# harness
against a synthetic BinaryReader; the surrounding action class and session are not compiled.
Every extracted source is hashed into the fixture header. Output is byte-identical across runs.
"""
from pathlib import Path
import hashlib, os, re, subprocess, tempfile

ROOT = Path(__file__).resolve().parents[4]
SRC = ROOT / '.reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b/Source'
ACTIONS = SRC / 'ACE.Server/Network/GameAction/Actions'
DOTNET = os.environ.get('BACE_DOTNET', '/home/jokerfactor/.dotnet/dotnet')
FIXTURE = ROOT / 'crates/network/bace-wire/tests/fixtures/channels.csv'

list_src = (ACTIONS / 'GameActionChannelList.cs').read_text()
index_src = (ACTIONS / 'GameActionChannelIndex.cs').read_text()
enter_src = (ACTIONS / 'GameActionEnterPkLite.cs').read_text()

# Verbatim read statements (comments stripped, as in group_generate.py).
def reads(source):
    return [l.strip().split('//')[0] for l in source.splitlines() if 'message.Payload.Read' in l]

list_reads = reads(list_src)
index_reads = reads(index_src)
enter_reads = reads(enter_src)
assert list_reads == ['var chatChannelID = (Channel)message.Payload.ReadUInt32();'], list_reads
assert index_reads == [], index_reads          # IndexChannels reads no payload bytes
assert enter_reads == [], enter_reads          # EnterPkLite has no payload: no decoder

# (action name, hex payload). ListChannels reads one uint32 and ignores trailing bytes.
CASES = [
    ('ListChannels', '01000000'),
    ('ListChannels', 'FFFFFFFF'),
    ('ListChannels', '0100000005060708'),       # trailing bytes after the uint32
    ('ListChannels', '0100'),                   # truncated uint32
    ('ListChannels', ''),                       # empty payload
    ('IndexChannels', ''),
    ('IndexChannels', '0100000000'),            # ACE ignores any payload bytes
]

def case_block(name, hex_payload):
    if name == 'ListChannels':
        body = f'''var bytes = Unhex("{hex_payload}");
  var message = new ClientMessage {{ Payload = new BinaryReader(new MemoryStream(bytes)) }};
  try {{
    {list_reads[0]}
    Console.WriteLine("case,ListChannels,{hex_payload},"+message.Payload.BaseStream.Position+","+chatChannelID);
  }} catch (EndOfStreamException) {{
    Console.WriteLine("case,ListChannels,{hex_payload},0,EndOfStream");
  }}'''
    else:
        # No verbatim reads: the consumed position is the stream position before any read (0).
        body = f'''var bytes = Unhex("{hex_payload}");
  var message = new ClientMessage {{ Payload = new BinaryReader(new MemoryStream(bytes)) }};
  Console.WriteLine("case,IndexChannels,{hex_payload},"+message.Payload.BaseStream.Position+",none");'''
    return '{\n  ' + body + '\n  }'

cases_code = '\n  '.join(case_block(n, h) for n, h in CASES)

HARNESS = '''using System;
using System.IO;

public enum Channel : uint { None = 0 }
public class ClientMessage { public BinaryReader Payload; }

class Program
{
  static byte[] Unhex(string hex)
  {
    var result = new byte[hex.Length / 2];
    for (int i = 0; i < result.Length; i++)
      result[i] = Convert.ToByte(hex.Substring(i * 2, 2), 16);
    return result;
  }

  static void Main()
  {
  ''' + cases_code + '''
  }
}
'''

with tempfile.TemporaryDirectory(prefix='channel-oracle-') as tmp:
    p = Path(tmp)
    (p / 'Program.cs').write_text(HARNESS)
    (p / 'oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>disable</ImplicitUsings><Nullable>disable</Nullable></PropertyGroup></Project>')
    subprocess.run([DOTNET, 'build', '--nologo', '-o', str(p / 'build')], cwd=p, check=True)
    output = subprocess.check_output([DOTNET, str(p / 'build/oracle.dll')], text=True)

sources = [ACTIONS / 'GameActionChannelList.cs', ACTIONS / 'GameActionChannelIndex.cs', ACTIONS / 'GameActionEnterPkLite.cs']
header = ''.join(
    '# ' + str(f.relative_to(SRC)) + ' ' + hashlib.sha256(f.read_bytes()).hexdigest() + '\n' for f in sources
)
FIXTURE.write_text(header + output)
print(FIXTURE)
