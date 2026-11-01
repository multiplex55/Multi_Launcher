//! Small incremental ZIP CRC-32 implementation shared by archive writing and
//! the existing radial package serializer.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Crc32(u32);

impl Crc32 {
    pub(crate) const fn new() -> Self {
        Self(!0_u32)
    }

    pub(crate) fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u32::from(*byte);
            for _ in 0..8 {
                self.0 = (self.0 >> 1) ^ (0xedb8_8320 & (0_u32.wrapping_sub(self.0 & 1)));
            }
        }
    }

    pub(crate) const fn finish(self) -> u32 {
        !self.0
    }
}

pub(crate) fn checksum(bytes: &[u8]) -> u32 {
    let mut crc = Crc32::new();
    crc.update(bytes);
    crc.finish()
}
