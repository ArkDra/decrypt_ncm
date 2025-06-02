use std::io::{Cursor, Read, Seek};

use crate::CORE_KEY;
use crate::META_KEY;
use crate::keybox::KeyBox;
use aes::{
    Aes128,
    cipher::{BlockDecryptMut, KeyInit, block_padding::Pkcs7, generic_array::GenericArray},
};
use base64::{Engine, prelude::BASE64_STANDARD};
use serde_json::Value;

pub fn aes_decrypt(mut data: Vec<u8>, key: &[u8]) -> Vec<u8> {
    let key = GenericArray::from_slice(key);
    let cipher = ecb::Decryptor::<Aes128>::new(key);
    let data = cipher.decrypt_padded_mut::<Pkcs7>(&mut data).unwrap();
    data.to_vec()
}

pub fn process_encrypted_data(data: Vec<u8>) -> (Vec<u8>, Vec<u8>, serde_json::Value) {
    let mut cursor = Cursor::new(data);

    // 处理文件头
    let mut header = [0u8; 8];
    let _ = cursor.read_exact(&mut header);

    // 验证是否是NCM文件
    if header != [0x43, 0x54, 0x45, 0x4E, 0x46, 0x44, 0x41, 0x4D] {
        panic!("无效的ncm文件！");
    }

    // 处理密钥数据
    cursor.seek(std::io::SeekFrom::Current(2)).unwrap();

    let mut key_length = [0u8; 4];
    let _ = cursor.read_exact(&mut key_length);
    let key_length = u32::from_le_bytes(key_length).try_into().unwrap();

    let mut key_data = vec![0u8; key_length];
    let _ = cursor.read_exact(&mut key_data);

    for i in 0..key_length {
        key_data[i] ^= 0x64;
    }

    let key_data = aes_decrypt(key_data, &CORE_KEY);
    let key_box = KeyBox::new(&key_data[17..]);

    // 处理元数据
    let mut meta_length = [0u8; 4];
    let _ = cursor.read_exact(&mut meta_length);
    let meta_length = u32::from_le_bytes(meta_length).try_into().unwrap();

    let mut meta_data = vec![0u8; meta_length];
    let _ = cursor.read_exact(&mut meta_data);

    for i in 0..meta_length {
        meta_data[i] ^= 0x63;
    }

    let meta_data = &meta_data[22..];
    let meta_data = BASE64_STANDARD.decode(meta_data).unwrap();
    let meta_data = aes_decrypt(meta_data, &META_KEY);
    let meta_data = &meta_data[6..];
    let meta_data: Value = serde_json::from_slice(&meta_data).unwrap();

    // 处理封面数据
    cursor.seek(std::io::SeekFrom::Current(9)).unwrap();

    let mut cover_length = [0u8; 4];
    let _ = cursor.read_exact(&mut cover_length);
    let cover_length = u32::from_le_bytes(cover_length).try_into().unwrap();

    let mut cover_data = vec![0u8; cover_length];
    let _ = cursor.read_exact(&mut cover_data);

    // 处理音乐数据
    let mut music_data = Vec::new();
    let _ = cursor.read_to_end(&mut music_data);
    let music_data = key_box.apply_keystream(music_data);

    (music_data, cover_data, meta_data)
}
