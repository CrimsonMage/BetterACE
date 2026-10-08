"""Compile local client JumpPack / CM_Movement methods; emit synthetic numeric bytes.
No decompile body or proprietary assets are copied into the repository.
"""
import argparse,hashlib,re,subprocess,tempfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);a=p.parse_args();root=Path(__file__).resolve().parent
jump=(a.source/'JumpPack.cpp').read_text();movement=(a.source/'CM_Movement.cpp').read_text()
def method(text,signature):
 start=text.index(signature);brace=text.index('{',start);depth=1;end=brace+1
 while depth:depth+=(text[end]=='{')-(text[end]=='}');end+=1
 return text[start:end]
pack=method(jump,'int __thiscall JumpPack::Pack(').replace('int __thiscall JumpPack::Pack(JumpPack *this,','int pack_jump(JumpPack *self,');pack=re.sub(r'\bthis\b','self',pack).replace('v9 = (char *)*addr + 4;','v9 = (_DWORD *)((char *)*addr + 4);')
nonauto=method(movement,'bool __cdecl CM_Movement::Event_Jump_NonAutonomous(').replace('bool __cdecl CM_Movement::Event_Jump_NonAutonomous(','bool pack_nonauto(').replace("(OrderHdr_vtbl *)&OrderHdr::`vftable'",'nullptr')
harness=(root/'physical_jump_harness.cpp').read_text().replace('// JUMP_METHOD',pack).replace('// NONAUTO_METHOD',nonauto)
with tempfile.TemporaryDirectory(prefix='betterace-jump-') as tmp:
 d=Path(tmp);(d/'oracle.cpp').write_text(harness);subprocess.run(['g++','-std=c++20','-fpermissive','-w','-O0','-fno-fast-math','-ffp-contract=off',str(d/'oracle.cpp'),'-o',str(d/'oracle')],check=True);output=subprocess.check_output([str(d/'oracle')],text=True)
headers=['# Local client decompile build provenance unconfirmed; numeric synthetic observations only', '# JumpPack::Pack0x00516D10; CM_Movement::Event_Jump_NonAutonomous0x006AFB30', '# Original method bodies compiled with Position and UI transport adapters; signature/vtable and one missing pointer-cast syntax adapted']
for name in ['JumpPack.cpp','CM_Movement.cpp','Position.cpp','Frame.cpp']:headers.append('# '+name+' sha256 '+hashlib.sha256((a.source/name).read_bytes()).hexdigest())
(root.parent/'tests/fixtures/physical_jump.csv').write_text('\n'.join(headers)+'\n'+output)
