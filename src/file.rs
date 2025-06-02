use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::Path;

pub fn is_ncm_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("ncm"))
        .unwrap_or(false)
}

pub fn read_file(file_path: &str) -> Vec<u8> {
    let file = File::open(file_path).unwrap();
    let mut buf = BufReader::new(file);
    let mut data = Vec::new();
    let _ = buf.read_to_end(&mut data).unwrap();
    data
}

pub fn write_file(file_path: &str, data: Vec<u8>) {
    let mut file = File::create(file_path).unwrap();
    let _ = file.write_all(&data);
}
