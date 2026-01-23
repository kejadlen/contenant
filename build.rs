use sha2::Digest;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo::rerun-if-changed=image/");

    let image_dir = Path::new("image");
    let mut hasher = sha2::Sha256::new();

    // Hash files in sorted order for deterministic output
    let mut files: Vec<_> = fs::read_dir(image_dir)
        .expect("Failed to read image directory")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .collect();
    files.sort_by_key(|e| e.path());

    for entry in files {
        let path = entry.path();
        let contents = fs::read(&path).expect("Failed to read file");
        sha2::Digest::update(&mut hasher, &contents);
    }

    let hash = format!("{:x}", sha2::Digest::finalize(hasher));
    let short_hash = &hash[..12];

    println!("cargo::rustc-env=IMAGE_HASH={}", short_hash);
}
