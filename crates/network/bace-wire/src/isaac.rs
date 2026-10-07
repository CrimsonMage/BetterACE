// Algorithm ported from official ACE.Common/Cryptography/ISAAC.cs at
// 47edade3bd3f6044b676d4eb877c4965c7eda62b. AGPL-3.0-only.
use std::{collections::BTreeSet, num::Wrapping};
type W = Wrapping<u32>;
#[derive(Clone)]
pub struct Isaac {
    offset: usize,
    a: W,
    b: W,
    c: W,
    memory: [W; 256],
    results: [W; 256],
}
impl Isaac {
    pub fn new(seed: u32) -> Self {
        let mut state = Self {
            offset: 255,
            a: Wrapping(seed),
            b: Wrapping(seed),
            c: Wrapping(seed),
            memory: [Wrapping(0); 256],
            results: [Wrapping(0); 256],
        };
        let mut mix = [Wrapping(0x9e3779b9); 8];
        for _ in 0..4 {
            shuffle(&mut mix);
        }
        for pass in 0..2 {
            for start in (0..256).step_by(8) {
                for (index, value) in mix.iter_mut().enumerate() {
                    *value += if pass == 0 {
                        state.results[start + index]
                    } else {
                        state.memory[start + index]
                    };
                }
                shuffle(&mut mix);
                state.memory[start..start + 8].copy_from_slice(&mix);
            }
        }
        state.scramble();
        state
    }
    pub fn next_key(&mut self) -> u32 {
        let value = self.results[self.offset].0;
        if self.offset > 0 {
            self.offset -= 1;
        } else {
            self.scramble();
            self.offset = 255;
        }
        value
    }
    fn scramble(&mut self) {
        self.c += Wrapping(1);
        self.b += self.c;
        for index in 0..256 {
            let x = self.memory[index];
            self.a ^= match index & 3 {
                0 => self.a << 13,
                1 => self.a >> 6,
                2 => self.a << 2,
                _ => self.a >> 16,
            };
            self.a += self.memory[(index + 128) & 255];
            let y = self.memory[((x.0 >> 2) & 255) as usize] + self.a + self.b;
            self.memory[index] = y;
            self.b = self.memory[((y.0 >> 10) & 255) as usize] + x;
            self.results[index] = self.b;
        }
    }
}
fn shuffle(x: &mut [W; 8]) {
    x[0] ^= x[1] << 11;
    x[3] += x[0];
    x[1] += x[2];
    x[1] ^= x[2] >> 2;
    x[4] += x[1];
    x[2] += x[3];
    x[2] ^= x[3] << 8;
    x[5] += x[2];
    x[3] += x[4];
    x[3] ^= x[4] >> 16;
    x[6] += x[3];
    x[4] += x[5];
    x[4] ^= x[5] << 10;
    x[7] += x[4];
    x[5] += x[6];
    x[5] ^= x[6] >> 4;
    x[0] += x[5];
    x[6] += x[7];
    x[6] ^= x[7] << 8;
    x[1] += x[6];
    x[7] += x[0];
    x[7] ^= x[0] >> 9;
    x[2] += x[7];
    x[0] += x[1];
}
/// ACE's bounded incoming checksum key search. Search intentionally advances
/// the key stream even on failure, matching CryptoSystem.Search.
#[derive(Clone)]
pub struct ClientKeys {
    isaac: Isaac,
    current: u32,
    unused: BTreeSet<u32>,
}
impl ClientKeys {
    pub fn new(seed: u32) -> Self {
        let mut isaac = Isaac::new(seed);
        let current = isaac.next_key();
        Self {
            isaac,
            current,
            unused: BTreeSet::new(),
        }
    }
    pub fn verify_and_consume(&mut self, key: u32) -> bool {
        let found = if self.current == key || self.unused.contains(&key) {
            true
        } else {
            let mut found = false;
            for _ in 0..256usize.saturating_sub(self.unused.len()) {
                self.unused.insert(self.current);
                self.current = self.isaac.next_key();
                if self.current == key {
                    found = true;
                    break;
                }
            }
            found
        };
        if found {
            if self.current == key {
                self.current = self.isaac.next_key();
            } else {
                self.unused.remove(&key);
            }
        }
        found
    }
}
