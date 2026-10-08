use hmac::{Hmac, Mac};
use sha2::Sha256;

/// Frozen algorithm-v1 domain tags. Never renumber or reuse a tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Domain {
    OrdinaryLoot = 1,
    RareOccurrence = 2,
    RareRealtime = 3,
    RareTier = 4,
    RareItem = 5,
    RareTimer = 6,
    Magic = 7,
    Combat = 8,
    Npc = 9,
    Aetheria = 10,
    Crafting = 11,
    Generator = 12,
    Recall = 13,
    Social = 14,
    PlayerDeath = 15,
}
impl Domain {
    fn rare(self) -> bool {
        (2..=6).contains(&(self as u16))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandomError {
    Domain,
    Identity,
    Label,
    Bound,
    Probability,
    Exhausted,
    DrawBudget,
}

/// Secret material intentionally has no Debug/serialization implementation.
pub struct RandomRoot {
    key: [u8; 32],
    version: u32,
}
impl RandomRoot {
    pub fn new(key: [u8; 32], version: u32) -> Result<Self, RandomError> {
        if version == 0 {
            return Err(RandomError::Identity);
        }
        Ok(Self { key, version })
    }
    pub fn key_version(&self) -> u32 {
        self.version
    }
    /// Character-local crafting scope. Both identifiers are server-owned and
    /// frozen with the durable proposal. Reconstructing it cannot reroll a retry.
    pub fn crafting_stream(
        &self,
        character: [u8; 16],
        operation: [u8; 16],
    ) -> Result<RandomStream, RandomError> {
        if character == [0; 16] {
            return Err(RandomError::Identity);
        }
        self.event_stream(operation, Domain::Crafting)?
            .fork(&character, 0)
    }
    /// Stable bootstrap identity for an existing native character without rare
    /// state. Reconnect before its first committed check cannot choose a new seed.
    pub fn character_identity(&self, character: u32) -> Result<[u8; 16], RandomError> {
        if !(0x5000_0001..=0x5fff_ffff).contains(&character) {
            return Err(RandomError::Identity);
        }
        let mut mac = keyed(&self.key);
        mac.update(b"BetterACE.random.character.v1\0");
        mac.update(&self.version.to_le_bytes());
        mac.update(&character.to_le_bytes());
        Ok(mac.finalize().into_bytes()[..16]
            .try_into()
            .expect("SHA256 prefix"))
    }
    pub fn event_stream(
        &self,
        event: [u8; 16],
        domain: Domain,
    ) -> Result<RandomStream, RandomError> {
        if domain.rare() {
            return Err(RandomError::Domain);
        }
        self.scope(0, event, 0, domain)
    }
    pub fn rare_stream(
        &self,
        character: [u8; 16],
        ordinal: u64,
        domain: Domain,
    ) -> Result<RandomStream, RandomError> {
        if !domain.rare() {
            return Err(RandomError::Domain);
        }
        self.scope(1, character, ordinal, domain)
    }
    fn scope(
        &self,
        kind: u8,
        identity: [u8; 16],
        ordinal: u64,
        domain: Domain,
    ) -> Result<RandomStream, RandomError> {
        if identity == [0; 16] {
            return Err(RandomError::Identity);
        }
        let mut mac = keyed(&self.key);
        mac.update(b"BetterACE.random.scope.v1\0");
        mac.update(&self.version.to_le_bytes());
        mac.update(&[kind]);
        mac.update(&(domain as u16).to_le_bytes());
        mac.update(&identity);
        mac.update(&ordinal.to_le_bytes());
        Ok(RandomStream {
            key: mac.finalize().into_bytes().into(),
            counter: 0,
        })
    }
}

/// Event-local cursor. Forks depend on labels/occurrences, not the parent's draw
/// position. A frozen event descriptor can reconstruct the same stream on retry.
#[derive(Clone)]
pub struct RandomStream {
    key: [u8; 32],
    counter: u64,
}
impl RandomStream {
    /// Algorithm-v1 cursor for durable event continuations. Never contains key material.
    pub fn position(&self) -> u64 {
        self.counter
    }
    /// Restore a previously frozen cursor on the same derived event stream.
    /// The workflow/rare owner must fence checkpoint revisions against rewind.
    pub fn seek(&mut self, position: u64) {
        self.counter = position;
    }

    pub fn fork(&self, label: &[u8], occurrence: u64) -> Result<Self, RandomError> {
        if label.is_empty() || label.len() > 128 {
            return Err(RandomError::Label);
        }
        let mut mac = keyed(&self.key);
        mac.update(b"BetterACE.random.fork.v1\0");
        mac.update(&(label.len() as u16).to_le_bytes());
        mac.update(label);
        mac.update(&occurrence.to_le_bytes());
        Ok(Self {
            key: mac.finalize().into_bytes().into(),
            counter: 0,
        })
    }
    pub fn next_u64(&mut self) -> Result<u64, RandomError> {
        let next = self.counter.checked_add(1).ok_or(RandomError::Exhausted)?;
        let mut mac = keyed(&self.key);
        mac.update(b"BetterACE.random.draw.v1\0");
        mac.update(&self.counter.to_le_bytes());
        let bytes = mac.finalize().into_bytes();
        self.counter = next;
        Ok(u64::from_le_bytes(
            bytes[..8].try_into().expect("SHA256 prefix"),
        ))
    }
    /// Uniform [0,upper), with no modulo bias or fallback on budget exhaustion.
    pub fn below(&mut self, upper: u64) -> Result<u64, RandomError> {
        if upper == 0 {
            return Err(RandomError::Bound);
        }
        let threshold = 0_u64.wrapping_sub(upper) % upper;
        for _ in 0..64 {
            let value = self.next_u64()?;
            if value >= threshold {
                return Ok(value % upper);
            }
        }
        Err(RandomError::DrawBudget)
    }
    /// Exact rational Bernoulli check; the endpoints do not consume draws.
    pub fn chance(&mut self, numerator: u64, denominator: u64) -> Result<bool, RandomError> {
        if denominator == 0 || numerator > denominator {
            return Err(RandomError::Probability);
        }
        if numerator == 0 {
            return Ok(false);
        }
        if numerator == denominator {
            return Ok(true);
        }
        Ok(self.below(denominator)? < numerator)
    }
}
fn keyed(key: &[u8; 32]) -> Hmac<Sha256> {
    Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts a 32-byte key")
}
