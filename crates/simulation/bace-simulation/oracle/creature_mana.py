#!/usr/bin/env python3
"""Original GDLE creature windup and release mana calls; not effect/formula parity.

The full CreatureBeginCast, SpellCastData defaults and LaunchSpellEffect prefix
are extracted unchanged. Synthetic helpers guarantee successful resolution,
target validity and skill, stationary poses and empty components. GenerateManaCost
returns an explicit independent input. AdjustMana records requested and clamped
applied deltas; that adapter is not a separate source-compatibility claim.
"""
import argparse
import hashlib
import os
from pathlib import Path
import shlex
import subprocess
import tempfile

PIN = "353cbab52ef7da2b7063bc3e3f008461d8531693"


def block(source, signature):
    start = source.index(signature)
    opening = source.index("{", start)
    depth = 0
    for end in range(opening, len(source)):
        depth += (source[end] == "{") - (source[end] == "}")
        if depth == 0:
            return source[start:end + 1]
    raise ValueError(f"unterminated source block: {signature}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=os.environ.get("BACE_GDLE_SOURCE"))
    parser.add_argument("--cxx", default=os.environ.get("CXX", "c++"))
    parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parents[1] / "tests/fixtures/creature_mana.csv")
    args = parser.parse_args()
    if args.source is None:
        parser.error("provide --source or BACE_GDLE_SOURCE with the pinned GDLE checkout")
    sources = {}
    for relative in ("Source/SpellcastingManager.cpp", "Source/SpellcastingManager.h", "Source/PhatSDK/GameEnums.h"):
        data = (args.source / relative).read_bytes()
        pinned = subprocess.check_output(["git", "-C", str(args.source), "show", f"{PIN}:{relative}"])
        if data != pinned:
            raise ValueError(f"source differs from pinned commit: {relative}")
        sources[relative] = data
    cpp = sources["Source/SpellcastingManager.cpp"].decode("utf-8-sig")
    header = sources["Source/SpellcastingManager.h"].decode("utf-8-sig")
    enums = sources["Source/PhatSDK/GameEnums.h"].decode("utf-8-sig")
    begin = block(cpp, "int CSpellcastingManager::CreatureBeginCast(")
    launch_start = cpp.index("int CSpellcastingManager::LaunchSpellEffect(")
    launch_end = cpp.index("\n\tCWeenieObject *target = GetCastTarget();", launch_start)
    # Only the suffix is supplied: no real effect, target PK check or effect result.
    launch = cpp[launch_start:launch_end] + "\nreturn WERROR_NONE;\n}\n"
    program = HARNESS.replace("// SOURCE_ERRORS", block(enums, "enum WErrorType") + ";")
    program = program.replace("// SOURCE_DATA", block(header, "struct SpellCastData") + ";")
    program = program.replace("// SOURCE_METHODS", begin + "\n" + launch)
    with tempfile.TemporaryDirectory(prefix="bace-creature-mana-") as directory:
        directory = Path(directory)
        source = directory / "oracle.cpp"
        executable = directory / ("oracle.exe" if os.name == "nt" else "oracle")
        source.write_text(program)
        subprocess.run(shlex.split(args.cxx) + ["-std=c++17", "-O0", "-fno-fast-math", "-ffp-contract=off", str(source), "-o", str(executable)], check=True)
        output = subprocess.check_output([str(executable)], text=True)
    rows = output.splitlines()
    if not 0 < len(rows) <= 2048 or any(len(row.split(",")) != 18 for row in rows):
        raise ValueError("oracle output schema or case bound")
    comments = [
        f"# GDLE {PIN}; AGPL-3.0-only, GDLE contributors",
        *[f"# sha256 {hashlib.sha256(data).hexdigest()} {relative}" for relative, data in sources.items()],
        "# Unchanged whole CreatureBeginCast and LaunchSpellEffect prefix before actual target/effect execution.",
        "# Synthetic helpers: successful resolve/validity/skill, no movement, empty components, supplied GenerateManaCost, recorded clamped AdjustMana.",
        "# Source double debit is preserved, not corrected here. launch_result=-1 means no windup was admitted; absent adjustment slots are zero.",
        "# player,ai_uses_mana,base_mana,current_mana,generated_mana,heal_at_max,locked,begin_result,launch_result,mana_after_begin,mana_after_launch,begin_motion_calls,mana_cost_calls,adjust_count,adjust_first,adjust_second,applied_first,applied_second",
    ]
    args.output.write_text("\n".join(comments) + "\n" + output)
    print(f"wrote {len(rows)} original-source vectors to {args.output}")


HARNESS = r'''
#include <algorithm>
#include <cfloat>
#include <cstdint>
#include <iostream>
#include <map>
#include <set>
#include <stdexcept>
#include <string>
#include <vector>
using std::min;
using WORD = uint16_t;
// SOURCE_ERRORS
// IDs below are adapter-local labels, not a wire/property-ID qualification.
constexpr int AI_USES_MANA_BOOL=1, PS_Fizzle=2, LTT_MAGIC=3, LTT_ERROR=4;
constexpr int SPELL_COMPONENT_DID=5, ITEM_CUR_MANA_INT=6;
constexpr uint32_t Motion_CastSpell=0x400000d3;
#define SERVER_INFO std::cerr
struct Timer { static constexpr double cur_time=100.; };
struct Position { double distance(const Position&) const { return 0.; } };
struct SpellFormula { uint32_t GetPowerLevelOfPowerComponent() const { return 1; } };
enum class ProjectileType { Undef };
struct CSpellBase { int _base_mana=0,_category=0,_power=1; float _component_loss=0; };
// SOURCE_DATA
struct Qualities { void SetInt(int,int) { throw std::runtime_error("weapon outside fixture scope"); } };
struct CWeenieObject {
    bool player=false,ai_uses_mana=false;
    int mana=0;
    Position m_Position;
    Qualities m_Qualities;
    std::vector<int> requested,applied;
    uint32_t GetID() const { return 1; }
    uint32_t GetTopLevelID() const { return 1; }
    uint32_t GetWieldedCasterID() const { return 0; }
    bool InqBoolQuality(int,bool) const { return ai_uses_mana; }
    int GetMana() const { return mana; }
    int GetHealth() const { return 100; }
    int GetMaxHealth() const { return 100; }
    int AdjustMana(int amount) {
        requested.push_back(amount);
        int before=mana; mana=std::clamp(mana+amount,0,1000);
        applied.push_back(mana-before); return mana-before;
    }
    CWeenieObject* AsPlayer() { return player?this:nullptr; }
    CWeenieObject* AsMeleeWeapon() { return nullptr; }
    CWeenieObject* AsMissileLauncher() { return nullptr; }
    std::string GetName() const { return "fixture"; }
    void SendText(const std::string&,int) {}
    void EmitEffect(int,double) {}
    int InqDIDQuality(int,int) { throw std::runtime_error("components outside scope"); }
    int InqIntQuality(int,int,int) { throw std::runtime_error("weapon outside scope"); }
    void DecrementStackOrStructureNum() { throw std::runtime_error("components outside scope"); }
};
struct SpellComponentBase { float _CDM=0; std::string _name; };
struct SpellComponentTable { const SpellComponentBase* InqSpellComponentBase(int) { throw std::runtime_error("components outside scope"); } };
struct MagicSystem { static SpellComponentTable* GetSpellComponentTable() { throw std::runtime_error("components outside scope"); } };
struct World { CWeenieObject* FindObject(uint32_t) { throw std::runtime_error("components outside scope"); } } world;
World* g_pWorld=&world;
enum class eRandomFormula { favorMid };
int getRandomNumber(int,int,eRandomFormula,double,int) { throw std::runtime_error("components outside scope"); }
template<class... Args> std::string csprintf(const char*,Args...) { return "fixture"; }
double GetMagicSkillChance(uint32_t,int) { return 1.; }
struct Random { static double RollDice(double,double) { return .5; } };
class CSpellcastingManager {
public:
    struct SpellCastingMotion { SpellCastingMotion(uint32_t,float,bool,bool,float) {} };
    CWeenieObject* m_pWeenie;
    bool m_bCasting=false,m_bTurningToObject=false;
    SpellCastData m_SpellCastData;
    std::vector<SpellCastingMotion> m_PendingMotions;
    std::map<uint32_t,uint32_t> m_UsedComponents;
    CSpellBase prepared;
    int generated=0,begin_calls=0,cost_calls=0;
    bool ResolveSpellBeingCasted() { m_SpellCastData.spell=&prepared; m_SpellCastData.current_skill=300; return true; }
    void BeginNextMotion() { ++begin_calls; }
    int CheckTargetValidity() { return 0; }
    CWeenieObject* GetCastTarget() { return m_pWeenie; }
    int GenerateManaCost() { ++cost_calls; return generated; }
    int CreatureBeginCast(uint32_t,uint32_t);
    int LaunchSpellEffect(bool,bool=false);
};
// SOURCE_METHODS
int main() {
    for(int player:{0,1}) for(int ai:{0,1}) for(int base:{0,1,10,100}) for(int generated:{0,3,10,12,100}) {
        std::set<int> currents{0,100,std::max(0,base-1),base,base+1,std::max(0,base+generated-1),base+generated,base+generated+1};
        for(int current:currents) for(int mode:{0,1,2}) {
            CWeenieObject actor; actor.player=player; actor.ai_uses_mana=ai; actor.mana=current;
            CSpellcastingManager manager; manager.m_pWeenie=&actor; manager.prepared._base_mana=base;
            manager.prepared._category=mode==1?67:0; manager.m_bCasting=mode==2; manager.generated=generated;
            int begin=manager.CreatureBeginCast(1,42),after_begin=actor.mana,launch=-1;
            if(manager.begin_calls) launch=manager.LaunchSpellEffect(false);
            if(actor.requested.size()>2 || actor.applied.size()!=actor.requested.size()) throw std::runtime_error("bounded debit trace");
            auto at=[](const std::vector<int>&v,size_t i) { return i<v.size()?v[i]:0; };
            std::cout<<player<<","<<ai<<","<<base<<","<<current<<","<<generated<<","<<(mode==1)<<","<<(mode==2)<<","<<begin<<","<<launch<<","<<after_begin<<","<<actor.mana<<","<<manager.begin_calls<<","<<manager.cost_calls<<","<<actor.requested.size()<<","<<at(actor.requested,0)<<","<<at(actor.requested,1)<<","<<at(actor.applied,0)<<","<<at(actor.applied,1)<<"\n";
        }
    }
}
'''

if __name__ == "__main__":
    main()
