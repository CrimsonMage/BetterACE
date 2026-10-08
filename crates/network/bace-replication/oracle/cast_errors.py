#!/usr/bin/env python3
"""Compile pinned GDLE error enum; handler routing provenance is TryBeginCast/CheckTargetValidity."""
import pathlib,argparse,tempfile,subprocess,hashlib
p=argparse.ArgumentParser();p.add_argument('--source',required=True);a=p.parse_args();root=pathlib.Path(a.source);pin='353cbab52ef7da2b7063bc3e3f008461d8531693';assert subprocess.check_output(['git','-C',str(root),'rev-parse','HEAD'],text=True).strip()==pin
path=root/'Source/PhatSDK/GameEnums.h';raw=path.read_bytes();source=raw.decode();start=source.index('enum WErrorType');end=source.index('};',start)+2;enum=source[start:end]
names=['WERROR_NONE','WERROR_BAD_PARAM','WERROR_ACTIONS_LOCKED','WERROR_OBJECT_GONE','WERROR_CANT_GET_THERE','WERROR_DEAD','WERROR_TOO_FAR','WERROR_MAGIC_INVALID_SPELL_TYPE','WERROR_MAGIC_UNLEARNED_SPELL','WERROR_MAGIC_BAD_TARGET_TYPE','WERROR_MAGIC_MISSING_COMPONENTS','WERROR_MAGIC_MISSING_TARGET','WERROR_MAGIC_INSUFFICIENT_MANA','WERROR_MAGIC_FIZZLE','WERROR_MAGIC_GENERAL_FAILURE','WERROR_MAGIC_UNPREPARED','WERROR_MISSILE_OUT_OF_RANGE']
with tempfile.TemporaryDirectory() as tmp:
 d=pathlib.Path(tmp);(d/'main.cpp').write_text('#include <cstdio>\n'+enum+'\nint main(){'+''.join(f'printf("{name},%u\\n",unsigned({name}));' for name in names)+'}')
 subprocess.run(['c++',str(d/'main.cpp'),'-o',str(d/'oracle')],check=True);result=subprocess.check_output([str(d/'oracle')],text=True)
 (pathlib.Path(__file__).parent.parent/'tests/fixtures/cast_errors.csv').write_text(f'# GDLE {pin} GameEnums.h SHA256 {hashlib.sha256(raw).hexdigest()}\n'+result)
