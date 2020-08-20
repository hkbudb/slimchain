use sgx_rand::{Rng as _, SgxRng};

pub struct OsRng(SgxRng);

pub fn os_rng() -> OsRng {
    OsRng(SgxRng::new().expect("Failed to call initialize SgxRng"))
}

impl rand_core::RngCore for OsRng {
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.0.fill_bytes(dest)
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

impl rand_core::CryptoRng for OsRng {}
