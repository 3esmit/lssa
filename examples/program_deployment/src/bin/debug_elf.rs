use std::env;
use std::fs;
use risc0_binfmt::ProgramBinary;

fn main() {
    let path = env::args().nth(1).expect("Usage: debug_elf <path>");
    let data = fs::read(&path).expect("Failed to read file");
    println!("Read {} bytes from {}", data.len(), path);

    match ProgramBinary::decode(&data) {
        Ok(binary) => {
            println!("Decode successful");
            match binary.compute_image_id() {
                Ok(id) => println!("Image ID computed successfully: {:?}", id),
                Err(e) => println!("Compute Image ID failed: {:?}", e),
            }
        },
        Err(e) => println!("Decode failed: {:?}", e),
    }
}
