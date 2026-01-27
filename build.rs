use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

fn main() {
    let image_dir = Path::new("image");

    // Collect and sort all files for deterministic hashing
    let mut files: Vec<_> = fs::read_dir(image_dir)
        .expect("Failed to read image directory")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .collect();
    files.sort_by_key(|e| e.file_name());

    // Hash all file contents
    let mut hasher = Sha256::new();
    for entry in &files {
        let content = fs::read(entry.path()).expect("Failed to read file");
        hasher.update(&content);
        // Tell cargo to rerun if this file changes
        println!("cargo::rerun-if-changed={}", entry.path().display());
    }

    let hash = format!("{:x}", hasher.finalize());
    // Take first 12 chars like Docker short hashes
    let short_hash = &hash[..12];

    println!("cargo::rustc-env=IMAGE_HASH={}", short_hash);
}
