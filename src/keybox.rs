use rayon::{iter::{ParallelIterator}, slice::ParallelSliceMut};

pub struct KeyBox {
    box_data: [u8; 256],
}

impl KeyBox {
    pub fn new(key_data: &[u8]) -> Self {
        let mut key_box: [u8; 256] = std::array::from_fn(|i| i as u8);
        let mut last_byte: u8 = 0;
        let mut key_offset: u8 = 0;

        for i in 0..256 {
            let c: u8 = key_box[i]
                .wrapping_add(last_byte)
                .wrapping_add(key_data[key_offset as usize]);
            key_offset = (key_offset + 1) % key_data.len() as u8;

            key_box.swap(i, c as usize);
            last_byte = c;
        }

        KeyBox { box_data: key_box }
    }

    pub fn apply_keystream(&self, data: Vec<u8>) -> Vec<u8> {
        let chunk_size = 16384;

        let mut output = data.clone();
        output.par_chunks_mut(chunk_size).for_each(|chunk| {
            chunk.iter_mut().enumerate().for_each(|(idx, byte)| {
                let j = (idx + 1) & 0xFF;
                let value = self.box_data[j]
                    .wrapping_add(self.box_data[(self.box_data[j] as usize + j) & 0xFF]);
                *byte ^= self.box_data[value as usize & 0xFF];
            });
        });

        output
    }
}
