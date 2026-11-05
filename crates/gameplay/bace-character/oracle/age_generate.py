"""Compile the verbatim GDLE GetAgeString and emit tests/fixtures/age.csv.

The original function is extracted byte-for-byte from the pinned GDLE ClientEvents.cpp and
compiled with g++ in a tiny harness. Only the GDLE `csprintf` helper (Util.h) is shimmed.
The harness receives the signed `int` that GDLE itself would see, so values above
i32::MAX are recorded as `signed_wrap` rows rather than silently reinterpreted.
Output is deterministic: the same source and seed give byte-identical CSV.
"""
import argparse, hashlib, random, subprocess, tempfile
from pathlib import Path

GDLE_COMMIT = '353cbab52ef7da2b7063bc3e3f008461d8531693'
SEED = 0x0A6E2C
PREFIX = 'You have played for '
SIGNATURE = 'std::string GetAgeString(int age)'

parser = argparse.ArgumentParser()
parser.add_argument('--source', type=Path, required=True, help='gdlenhanced checkout at GDLE_COMMIT')
parser.add_argument('--gxx', default='g++')
args = parser.parse_args()

head = subprocess.check_output(['git', '-C', str(args.source), 'rev-parse', 'HEAD'], text=True).strip()
assert head == GDLE_COMMIT, f'GDLE checkout is {head}, expected {GDLE_COMMIT}'
client_events = args.source / 'Source' / 'ClientEvents.cpp'
data = client_events.read_bytes()
source = data.decode('utf-8-sig')
start = source.index(SIGNATURE)
open_brace = source.index('{', start)
depth = 0
for i in range(open_brace, len(source)):
    depth += (source[i] == '{') - (source[i] == '}')
    if depth == 0:
        verbatim = source[start:i + 1]
        break
else:
    raise ValueError('unterminated GetAgeString')

harness = '''#include <cstdarg>
#include <cstdio>
#include <iostream>
#include <string>
#include <vector>
// Shim for GDLE Util.h csprintf (static-buffer printf); the age function is verbatim.
static std::string csprintf(const char* format, ...) {
    char buffer[256];
    va_list args;
    va_start(args, format);
    std::vsnprintf(buffer, sizeof(buffer), format, args);
    va_end(args);
    return std::string(buffer);
}
''' + verbatim + '''
int main() {
    int value;
    while (std::cin >> value) {
        std::cout << GetAgeString(value) << "\\n";
    }
    return 0;
}
'''

# Fixed boundaries, then 200 seeded values: 100 below 2^31 and 100 across all of u32.
sweep = [0, 1, 59, 60, 3599, 3600, 86399, 86400,
         2591999, 2592000, 2592001,          # 30 days -/+ 1 s
         2629799, 2629800, 2629801,          # GDLE month constant -/+ 1 s
         31536000,                           # 365 days
         34560000,                           # 400 days (more than a year)
         2147483647, 2147483648,             # i32 boundary
         4294967295]                         # u32::MAX
rng = random.Random(SEED)
sweep += [rng.getrandbits(31) for _ in range(100)]
sweep += [rng.getrandbits(32) for _ in range(100)]


def gdle_int(seconds):
    return seconds - (1 << 32) if seconds >= (1 << 31) else seconds


with tempfile.TemporaryDirectory(prefix='bace-age-') as td:
    cpp = Path(td) / 'age_harness.cpp'
    exe = Path(td) / 'age_harness'
    cpp.write_text(harness)
    subprocess.run([args.gxx, '-std=c++17', '-O0', '-Wall', '-Werror', '-o', str(exe), str(cpp)], check=True)
    stdin = ''.join(f'{gdle_int(s)}\n' for s in sweep)
    lines = subprocess.run([str(exe)], input=stdin, text=True, capture_output=True, check=True).stdout.splitlines()

assert len(lines) == len(sweep)
rows = []
for seconds, text in zip(sweep, lines):
    assert text.startswith(PREFIX) and text.endswith('.'), text
    body = text[len(PREFIX):-1]
    assert ',' not in body and '\n' not in body
    g = gdle_int(seconds)
    status = 'exact' if g == seconds else 'signed_wrap'
    rows.append(f'{seconds},{g},{status},{body}')

provenance = [
    f'# GDLE commit {GDLE_COMMIT}',
    f'# sha256 {hashlib.sha256(data).hexdigest()} Source/ClientEvents.cpp GetAgeString',
    '# columns: seconds(u32),gdle_int(as GDLE int),status,body(GetAgeString without prefix and period)',
    '# seed ' + hex(SEED) + ' ; 200 seeded values appended after the fixed boundaries',
]
out = Path(__file__).parent.parent / 'tests' / 'fixtures' / 'age.csv'
out.write_text('\n'.join(provenance + rows) + '\n')
print(f'wrote {len(rows)} rows to {out}')
