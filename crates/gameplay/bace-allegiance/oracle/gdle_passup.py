#!/usr/bin/env python3
"""Compile the original pinned GDLE pass-up expression, not a port of our Rust.
Requires a local licensed upstream checkout; writes only generated numeric CSV.
GDLE Source/AllegianceManager.cpp, AGPL-compatible upstream attribution retained.
"""
import argparse
import csv
import hashlib
import pathlib
import subprocess
import tempfile

PIN = '353cbab52ef7da2b7063bc3e3f008461d8531693'
p = argparse.ArgumentParser()
p.add_argument('checkout', type=pathlib.Path)
p.add_argument('--output', type=pathlib.Path, default=pathlib.Path(__file__).parents[1] / 'tests/fixtures/gdle_passup.csv')
a = p.parse_args()
assert subprocess.check_output(['git', '-C', str(a.checkout), 'rev-parse', 'HEAD'], text=True).strip() == PIN
source = (a.checkout / 'Source/AllegianceManager.cpp').read_text()
start = source.index('\tdouble vassalFactor =', source.index('void AllegianceManager::HandleAllegiancePassup'))
end = source.index('\n\tif (passupAmount > 0)', start)
body = source[start:end]
harness = '''#include <algorithm>
#include <cstdint>
#include <iostream>
#include <vector>
using std::min; using std::max;
struct Node {uint32_t _loyalty; uint32_t _leadership; std::vector<int> _vassals;};
int main(){int direct;int64_t amount;size_t count;double realDaysSworn,ingameHoursSworn,avgRealDaysVassalsSworn,avgIngameHoursVassalsSworn;Node n,p;Node*node=&n;Node*patron=&p;
while(std::cin>>amount>>direct>>n._loyalty>>p._leadership>>realDaysSworn>>ingameHoursSworn>>avgRealDaysVassalsSworn>>avgIngameHoursVassalsSworn>>count){p._vassals.resize(count);
''' + body + '\nstd::cout<<passupAmount<<"\\n";}}'
rows=[]
for direct in (0,1):
    for skill in (0,145,290,291,1000):
        for days,hours in ((0,0),(365,360),(730,720),(5000,5000),(1,0.08333333333333333)):
            for vassals in (1,3,4,11):
                rows.append((10000003,direct,skill,291-skill if skill<=291 else 1000,days,hours,days*1.5,hours*1.5,vassals))
rows += [(1,1,291,291,730,720,730,720,4),(9223372036854770000,1,291,291,730,720,730,720,11)]
with tempfile.TemporaryDirectory(prefix='bace-gdle-passup-') as tmp:
    tmp=pathlib.Path(tmp)
    (tmp/'main.cpp').write_text(harness)
    subprocess.run(['g++','-std=c++17','-O0','-fno-fast-math',str(tmp/'main.cpp'),'-o',str(tmp/'oracle')],check=True)
    answers=subprocess.check_output([str(tmp/'oracle')],input=''.join(' '.join(map(str,row))+'\n' for row in rows),text=True).splitlines()
assert len(rows)==len(answers)
a.output.parent.mkdir(parents=True,exist_ok=True)
with a.output.open('w') as f:
    f.write(f'# GDLE {PIN} Source/AllegianceManager.cpp extracted-sha256={hashlib.sha256(body.encode()).hexdigest()}\n')
    writer=csv.writer(f,lineterminator='\n')
    writer.writerow(('amount','direct','loyalty','leadership','real_days','game_hours','avg_real_days','avg_game_hours','vassals','passup'))
    writer.writerows((*row,answer) for row,answer in zip(rows,answers))
print(f'Wrote {len(rows)} original C++ vectors to {a.output}')
