mod decrypt;
mod file;
mod keybox;
mod metadata;
use crate::decrypt::*;
use crate::file::*;
use crate::metadata::*;

use rayon::iter::IntoParallelRefIterator;
use rayon::prelude::ParallelIterator;
use std::fs;
use std::path::Path;

const CORE_KEY: [u8; 16] = [
    0x68, 0x7a, 0x48, 0x52, 0x41, 0x6d, 0x73, 0x6f, 0x35, 0x6b, 0x49, 0x6e, 0x62, 0x61, 0x78, 0x57,
];
const META_KEY: [u8; 16] = [
    0x23, 0x31, 0x34, 0x6C, 0x6A, 0x6B, 0x5F, 0x21, 0x5C, 0x5D, 0x26, 0x30, 0x55, 0x3C, 0x27, 0x28,
];

fn process_input(input_path: &str) {
    let path = Path::new(input_path);

    if !path.exists() {
        eprintln!("路径不存在！");
        return;
    }

    if path.is_file() {
        if is_ncm_file(path) {
            decrypt_file(path);
        } else {
            eprintln!("提供的文件不是.ncm格式！");
        }
    } else if path.is_dir() {
        let ncm_files: Vec<_> = fs::read_dir(path)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|file_path| is_ncm_file(file_path))
            .collect();

        ncm_files.par_iter().for_each(|file_path| {
            decrypt_file(file_path);
        });
    } else {
        eprintln!("无效的路径类型！");
    }
}

fn decrypt_file(file_path: &Path) {
    let file_data = read_file(file_path.to_str().unwrap());

    let (music_data, cover_data, meta_data) = process_encrypted_data(file_data);

    let music_name = meta_data["musicName"].as_str().unwrap();
    let ext = meta_data["format"].as_str().unwrap();
    let artists = process_artist(&meta_data);
    let file_name = format!("{} - {}.{}", music_name, artists.as_str(), ext);
    let output_path = file_path.with_file_name(file_name);

    write_file(output_path.to_str().unwrap(), music_data);
    add_meta_info(output_path.to_str().unwrap(), &meta_data, cover_data);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("请提供文件或文件夹路径！");
        return;
    }

    let input_path = &args[1];
    process_input(input_path);
}
