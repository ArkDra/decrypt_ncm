use anyhow::{Result, bail};

pub struct KeyBox {
    keystream: [u8; 256],
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

        // The NCM keystream repeats every 256 bytes.
        let keystream = std::array::from_fn(|i| {
            let j = (i + 1) & 0xFF;
            let value = key_box[j].wrapping_add(key_box[(key_box[j] as usize + j) & 0xFF]);
            key_box[value as usize]
        });
        Ok(KeyBox { keystream })
    }

    pub fn apply_keystream(&self, data: &mut [u8], offset: u64) {
        let start = (offset & 0xFF) as usize;
        for (index, byte) in data.iter_mut().enumerate() {
            *byte ^= self.keystream[(start + index) & 0xFF];
        }
    }
}
