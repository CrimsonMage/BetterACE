#!/usr/bin/env python3
"""Execute pinned GDLE's original integer PK deadline/update/check/clear methods."""
import argparse
import hashlib
import pathlib
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument("--source", required=True)
args = parser.parse_args()
pin = "353cbab52ef7da2b7063bc3e3f008461d8531693"
root = pathlib.Path(args.source)

def source(path):
    return subprocess.check_output(["git", "-C", str(root), "show", f"{pin}:{path}"])

raw = source("Source/Player.cpp")
header = source("Source/Player.h")
text = raw.decode()
start = text.index("void CPlayerWeenie::UpdatePKActivity()")
update = text[start:text.index("\n}", start) + 2]
methods = "\n".join(line for line in header.decode().splitlines()
                    if "bool CheckPKActivity()" in line or "void ClearPKActivity()" in line)
code = """
#include <cstdio>
struct Timer { static double cur_time; }; double Timer::cur_time = 0;
constexpr int LAST_PK_ATTACK_TIMESTAMP_FLOAT = 1;
struct Qualities { double stored; void SetFloat(int, double v) { stored = v; } };
struct CPlayerWeenie { int m_iPKActivity = 0; Qualities m_Qualities;
void UpdatePKActivity();
""" + methods + "\n};\n" + update + """
int main() {
  for (double now : {0., .999, 1., 10.25, 1000000.999, 2147483600.}) {
    CPlayerWeenie player; Timer::cur_time = now; player.UpdatePKActivity();
    const double deadline = player.m_Qualities.stored;
    for (double delta : {-.001, 0., .001}) {
      Timer::cur_time = deadline + delta;
      const bool active = player.CheckPKActivity();
      player.ClearPKActivity();
      printf("%.17g,%.17g,%.17g,%d,%d\\n", now, deadline, Timer::cur_time, active, player.CheckPKActivity());
      Timer::cur_time = now; player.UpdatePKActivity();
    }
  }
}
"""
code = "#include <initializer_list>\n" + code
with tempfile.TemporaryDirectory() as directory:
    directory = pathlib.Path(directory)
    (directory / "oracle.cpp").write_text(code)
    subprocess.run(["c++", "-std=c++17", str(directory / "oracle.cpp"), "-o", str(directory / "oracle")], check=True)
    output = subprocess.check_output([str(directory / "oracle")], text=True)
    destination = pathlib.Path(__file__).parent.parent / "tests/fixtures/gdle_pk_activity.csv"
    destination.write_text(f"# GDLE {pin}\n# Player.cpp sha256 {hashlib.sha256(raw).hexdigest()}\n# Player.h sha256 {hashlib.sha256(header).hexdigest()}\n# update_time,deadline,check_time,active,cleared_active\n" + output)
