#!/usr/bin/env python3
"""Verbatim pinned GDLE double-cursor/hook oracle. No proprietary inputs."""
import argparse,hashlib,json,subprocess,tempfile
from pathlib import Path
PIN='353cbab52ef7da2b7063bc3e3f008461d8531693'
FILES=['Source/PhatSDK/'+x for x in ['PartArray.cpp','Animation.cpp','Animation.h','DLListBase.h','PhatSDK.h','MotionTable.cpp','PhysicsObj.cpp']]+['Source/SpellcastingManager.cpp']
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--cxx',default='g++');args=p.parse_args();root=Path(__file__).resolve().parent
sources={}
for path in FILES:
 data=(args.source/path).read_bytes();original=subprocess.check_output(['git','-C',str(args.source),'show',f'{PIN}:{path}'])
 if data!=original:raise ValueError('source differs from pin: '+path)
 sources[path]=data

def method(text,signature):
 start=text.index(signature);brace=text.index('{',start);depth=1;i=brace+1
 while depth:depth+=(text[i]=='{')-(text[i]=='}');i+=1
 return text[start:i]
part=sources['Source/PhatSDK/PartArray.cpp'].decode();animation=sources['Source/PhatSDK/Animation.cpp'].decode()
code='\n'.join(method(part,'void CSequence::'+name+'(') for name in ['update_internal','advance_to_next_animation','execute_hooks','apricot'])
for signature in ['float AnimSequenceNode::get_framerate(','float AnimSequenceNode::get_starting_frame(','float AnimSequenceNode::get_ending_frame(','int32_t AnimSequenceNode::get_low_frame(','int32_t AnimSequenceNode::get_high_frame(','AnimSequenceNode *AnimSequenceNode::GetNext(','AnimSequenceNode *AnimSequenceNode::GetPrev(','AnimFrame *AnimSequenceNode::get_part_frame(']:code+='\n'+method(animation,signature)
motion=sources['Source/PhatSDK/MotionTable.cpp'].decode()
code+='\n'+method(motion,'MotionData *CMotionTable::get_link(')+method(motion,'void MotionTableManager::CheckForCompletedMotions(')
code+='\n'+method(motion,'BOOL CMotionTable::is_allowed(')+method(motion,'BOOL same_sign(')+method(motion,'void change_cycle_speed(')
branch=method(motion,'\tif (motionid & CM_SubState)')+method(motion,'\tif (motionid & CM_Action) // CM_Action')
code+='\nBOOL CMotionTable::action(uint32_t motionid,MotionState *curr_state,CSequence *sequence,float speed_mod,uint32_t *num_anims){*num_anims=0;if(!curr_state->style||!curr_state->substate)return FALSE;MotionData *var_10=nullptr;uint32_t mtype2=curr_state->substate,new_substate=0;'+branch+'return FALSE;}'
harness=(root/'gdle_sequence_harness.cpp').read_bytes()
with tempfile.TemporaryDirectory(prefix='bace-gdle-sequence-') as temp:
 build=Path(temp);(build/'DLListBase.h').write_bytes(sources['Source/PhatSDK/DLListBase.h']);(build/'methods.inc').write_text(code);(build/'main.cpp').write_bytes(harness)
 subprocess.run([args.cxx,'-std=c++17','-O2','-fno-fast-math','-ffp-contract=off',str(build/'main.cpp'),'-o',str(build/'oracle')],check=True)
 vectors=json.loads(subprocess.check_output([str(build/'oracle')],text=True))
 queue_code='\n'.join(method(part,'void CSequence::'+name+'(') for name in ['update_internal','advance_to_next_animation','execute_hooks','apricot','append_animation','remove_cyclic_anims','remove_link_animations','multiply_cyclic_animation_framerate'])
 for signature in ['float AnimSequenceNode::get_framerate(','float AnimSequenceNode::get_starting_frame(','float AnimSequenceNode::get_ending_frame(','int32_t AnimSequenceNode::get_low_frame(','int32_t AnimSequenceNode::get_high_frame(','AnimSequenceNode *AnimSequenceNode::GetNext(','AnimSequenceNode *AnimSequenceNode::GetPrev(','AnimFrame *AnimSequenceNode::get_part_frame(','void AnimSequenceNode::multiply_framerate(']:queue_code+='\n'+method(animation,signature)
 for name in ['CheckForCompletedMotions','AnimationDone','add_to_queue','remove_redundant_links','truncate_animation_list']:queue_code+='\n'+method(motion,'void MotionTableManager::'+name+'(')
 queue_harness=(root/'gdle_sequence_queue_harness.cpp').read_bytes()
 queue_code+='\nint source_cast_gate(CWeenie *m_pWeenie){'+method(sources['Source/SpellcastingManager.cpp'].decode(),'\tif (m_pWeenie->get_minterp()->interpreted_state.actions.size())')+'return 0;}'
 (build/'queue_methods.inc').write_text(queue_code);(build/'queue.cpp').write_bytes(queue_harness)
 subprocess.run([args.cxx,'-std=c++17','-O2','-fno-fast-math','-ffp-contract=off',str(build/'queue.cpp'),'-o',str(build/'queue')],check=True)
 vectors['queue']=json.loads(subprocess.check_output([str(build/'queue')],text=True))

fixture={'repository':'https://gitlab.com/Scribble/gdlenhanced','commit':PIN,'source_sha256':{p:hashlib.sha256(b).hexdigest() for p,b in sources.items()},'harness_sha256':hashlib.sha256(harness).hexdigest(),'queue_harness_sha256':hashlib.sha256(queue_harness).hexdigest(),'harness':'Verbatim CSequence progression/hooks/apricot and AnimSequenceNode accessors; original intrusive list; original get_link and GetObjectSequence action/substate branches and zero-link manager completion selection/order/count with lookup adapters. A separate original CSequence/MotionTableManager queue harness qualifies active-link retention, cyclic replacement/rate updates, FIFO completion and plain-substate compaction; synthetic lookup supplies frames and no root transform. Original TryBeginCast action-list gate uses an opaque error sentinel; numeric wire error IDs are not qualified by this harness. Synthetic node/frame/hook declarations. Root-transform methods abort if called: nullptr root explicitly qualifies timing/hooks only. Quantum passes float before promotion to double as PhysicsObj→PartArray does.','vectors':vectors}
(root.parent/'fixtures/gdle_sequence.json').write_text(json.dumps(fixture,indent=2)+'\n')
