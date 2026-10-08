# bace-ai

Awareness, behavior, navigation intent and combat pets.

Status: foundation; complete subsystem behavior is not implemented.

Implementation belongs in named modules. Crate roots remain declaration-only.

The bounded `Awareness` query implements the eligibility filters in pinned `Monster_Awareness.GetAttackTargets`: authoritative distance, separate chase/visual ranges, attackability, teleport state, faction/retaliation, locked targets and monster-only tolerance. Inputs are read projections, never client-selected accepted state. Output capacity is checked before publication. Movement, target-choice tactics, retaliation orchestration and combat behavior remain unsupported.

Source: ACEmulator/ACE `47edade3bd3f6044b676d4eb877c4965c7eda62b`, `Source/ACE.Server/WorldObjects/Monster_Awareness.cs`, AGPL-3.0-only, ACE contributors. Tests cover authority boundaries; no full AI parity claim.

Monster authority now also follows pinned GDLE `MonsterAI.cpp`/`MonsterAI.h`:
home is established from authoritative grounded physics, chase/home bounds gate
pursuit, home return uses legal motion, and timeout recovery requests an explicit
validated server teleport. `MonsterLeash` preserves the default 100 m chase,
150 m home range, 5 m arrival threshold and 30-second return timeout (900 ticks
at the required 30 Hz). Simulation uses GDLE's next-attack delay of 2 seconds plus
selected power. Target immunity/faction/tolerance modes, full navigation and
motion/attack animation selection remain unimplemented; current nearest-player
selection is an explicit synthetic integration policy, not full GDLE AI parity.
