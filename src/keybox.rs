use anyhow::{bail, Result};
use rayon::prelude::*;

const CHUNK_SIZE: usize = 16384;

pub struct KeyBox {
    box_data: [u8; 256],
}

impl KeyBox {
    pub fn new(key_data: &[u8]) -> Result<Self> {
        if key_data.is_empty() {
            bail!("key data is empty");
        }

        let mut key_box: [u8; 256] = std::array::from_fn(|i| i as u8);
        let mut last_byte: u8 = 0;
        let mut key_offset: usize = 0;

        for i in 0..256 {
            let c: u8 = key_box[i]
                .wrapping_add(last_byte)
                .wrapping_add(key_data[key_offset]);
            key_offset = (key_offset + 1) % key_data.len();

            key_box.swap(i, c as usize);
            last_byte = c;
        }

        Ok(KeyBox { box_data: key_box })
    }

    pub fn apply_keystream(&self, mut data: Vec<u8>) -> Vec<u8> {
        data.par_chunks_mut(CHUNK_SIZE)
            .enumerate()
            .for_each(|(chunk_idx, chunk)| {
                let global_offset = chunk_idx * CHUNK_SIZE;
                chunk.iter_mut().enumerate().for_each(|(local_idx, byte)| {
                    let global_idx = global_offset + local_idx;
                    let j = (global_idx + 1) & 0xFF;
                    let value = self.box_data[j]
                        .wrapping_add(self.box_data[(self.box_data[j] as usize + j) & 0xFF]);
                    *byte ^= self.box_data[value as usize & 0xFF];
                });
            });

        data
    }
}