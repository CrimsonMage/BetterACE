use postcard::{Error, Result, ser_flavors::Flavor};

/// Grows with the actual DTO, not its maximum permitted size; rejects before
/// appending beyond the envelope budget. Avoids a 16 MiB allocation per item.
pub(crate) struct BoundedOutput {
    bytes: Vec<u8>,
    limit: usize,
}
impl BoundedOutput {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }
}
impl Flavor for BoundedOutput {
    type Output = Vec<u8>;
    fn try_push(&mut self, data: u8) -> Result<()> {
        if self.bytes.len() >= self.limit {
            return Err(Error::SerializeBufferFull);
        }
        self.bytes.push(data);
        Ok(())
    }
    fn try_extend(&mut self, data: &[u8]) -> Result<()> {
        if data.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(Error::SerializeBufferFull);
        }
        self.bytes.extend_from_slice(data);
        Ok(())
    }
    fn finalize(self) -> Result<Self::Output> {
        Ok(self.bytes)
    }
}
