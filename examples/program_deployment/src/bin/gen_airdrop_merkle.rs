use clap::Parser;
use hex::{FromHex, ToHex};
use sha2::{Digest, Sha256};

const LEAF_TAG: &[u8] = b"AirdropTicket";

#[derive(Parser, Debug)]
#[command(author, version, about = "Generate Merkle root + paths for airdrop tickets")]
struct Args {
    /// Ticket secrets as hex (32 bytes). Can be repeated.
    #[arg(long = "ticket")]
    ticket_hex: Vec<String>,
    /// File with one 32-byte hex ticket secret per line.
    #[arg(long = "tickets-file")]
    tickets_file: Option<String>,
}

fn sha256_bytes(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for p in parts {
        hasher.update(p);
    }
    hasher.finalize().into()
}

fn parse_ticket(hex_str: &str) -> [u8; 32] {
    <[u8; 32]>::from_hex(hex_str.trim_start_matches("0x"))
        .expect("Ticket must be 32 bytes hex")
}

fn build_tree(mut leaves: Vec<[u8; 32]>) -> Vec<Vec<[u8; 32]>> {
    if leaves.is_empty() {
        panic!("No tickets provided");
    }
    let mut layers = Vec::new();
    layers.push(leaves.clone());
    while leaves.len() > 1 {
        let mut next = Vec::with_capacity((leaves.len() + 1) / 2);
        let mut i = 0;
        while i < leaves.len() {
            let left = leaves[i];
            let right = if i + 1 < leaves.len() {
                leaves[i + 1]
            } else {
                leaves[i] // duplicate last if odd
            };
            next.push(sha256_bytes(&[&left, &right]));
            i += 2;
        }
        layers.push(next.clone());
        leaves = next;
    }
    layers
}

fn merkle_path(layers: &[Vec<[u8; 32]>], mut index: usize) -> Vec<[u8; 32]> {
    let mut path = Vec::new();
    for layer in layers.iter().take(layers.len() - 1) {
        let sibling = if index % 2 == 0 {
            if index + 1 < layer.len() {
                layer[index + 1]
            } else {
                layer[index]
            }
        } else {
            layer[index - 1]
        };
        path.push(sibling);
        index /= 2;
    }
    path
}

fn read_tickets_from_file(path: &str) -> Vec<[u8; 32]> {
    let content = std::fs::read_to_string(path).expect("Failed to read tickets file");
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(parse_ticket)
        .collect()
}

fn main() {
    let args = Args::parse();

    let mut tickets: Vec<[u8; 32]> = Vec::new();
    if let Some(path) = args.tickets_file.as_deref() {
        tickets.extend(read_tickets_from_file(path));
    }
    tickets.extend(args.ticket_hex.iter().map(|t| parse_ticket(t)));

    if tickets.is_empty() {
        panic!("Provide at least one ticket via --ticket or --tickets-file");
    }

    let leaves: Vec<[u8; 32]> = tickets
        .iter()
        .map(|t| sha256_bytes(&[LEAF_TAG, t]))
        .collect();

    let layers = build_tree(leaves);
    let root = layers.last().unwrap()[0];
    println!("merkle_root={}", root.encode_hex::<String>());
    println!("note=odd leaves are duplicated when building the tree");

    for (i, ticket) in tickets.iter().enumerate() {
        let path = merkle_path(&layers, i);
        let path_csv = path
            .iter()
            .map(|p| p.encode_hex::<String>())
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "ticket_index={} ticket_secret={} path_csv={}",
            i,
            ticket.encode_hex::<String>(),
            path_csv
        );
    }
}
