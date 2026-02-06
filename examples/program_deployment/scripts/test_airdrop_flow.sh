#!/usr/bin/env bash
set -euo pipefail

# End-to-end airdrop test flow:
# 1) create issuer wallet
# 2) generate Merkle tree
# 3) create recipient wallets/accounts with ticket secrets
# 4) claim airdrop from each recipient

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
PROGRAM_DEPLOYMENT_DIR="$ROOT_DIR/examples/program_deployment"
BUILD_DIR="${EXAMPLE_PROGRAMS_BUILD_DIR:-$ROOT_DIR/target/riscv32im-risc0-zkvm-elf/docker}"
CONFIG_TEMPLATE="${WALLET_CONFIG_TEMPLATE:-$ROOT_DIR/integration_tests/configs/wallet/wallet_config.json}"
NUM_RECIPIENTS="${NUM_RECIPIENTS:-2}"
TEST_ROOT="${TEST_ROOT:-/tmp/lssa-airdrop-test-$(date +%s)}"
SEQUENCER_ADDR="${SEQUENCER_ADDR:-http://127.0.0.1:3040}"

ISSUER_PASSWORD="${ISSUER_PASSWORD:-issuer-pass}"
RECIPIENT_PASSWORD_PREFIX="${RECIPIENT_PASSWORD_PREFIX:-recipient-pass-}"

ISSUER_HOME="$TEST_ROOT/issuer"
RECIPIENTS_ROOT="$TEST_ROOT/recipients"
TMP_DIR="$TEST_ROOT/tmp"

mkdir -p "$TMP_DIR" "$RECIPIENTS_ROOT"

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing command: $1" >&2
    exit 1
  }
}

need_cmd wallet
need_cmd cargo
need_cmd openssl
need_cmd python3

if [[ ! -f "$CONFIG_TEMPLATE" ]]; then
  # Backward-compatible fallback locations.
  for candidate in \
    "$ROOT_DIR/integration_tests/configs/debug/wallet/wallet_config.json" \
    "$ROOT_DIR/integration_tests/configs/wallet/wallet_config.json"
  do
    if [[ -f "$candidate" ]]; then
      CONFIG_TEMPLATE="$candidate"
      break
    fi
  done
fi

if [[ ! -f "$CONFIG_TEMPLATE" ]]; then
  echo "wallet_config template not found." >&2
  echo "Set WALLET_CONFIG_TEMPLATE=/path/to/wallet_config.json and rerun." >&2
  exit 1
fi

if [[ ! -f "$BUILD_DIR/airdrop.bin" || ! -f "$BUILD_DIR/airdrop_ticket_init.bin" ]]; then
  echo "Missing guest binaries in $BUILD_DIR"
  echo "Run:"
  echo "  cargo risczero build --manifest-path examples/program_deployment/methods/guest/Cargo.toml"
  exit 1
fi

init_wallet_home() {
  local home_dir="$1"
  local password="$2"
  mkdir -p "$home_dir"
  cp "$CONFIG_TEMPLATE" "$home_dir/wallet_config.json"
  # Ensure a valid absolute sequencer URL in copied config.
  python3 - "$home_dir/wallet_config.json" "$SEQUENCER_ADDR" <<'PY'
import json, sys
cfg_path, sequencer_addr = sys.argv[1], sys.argv[2]
with open(cfg_path, "r", encoding="utf-8") as f:
    cfg = json.load(f)
cfg["sequencer_addr"] = sequencer_addr
with open(cfg_path, "w", encoding="utf-8") as f:
    json.dump(cfg, f, indent=2)
    f.write("\n")
PY
  # First command initializes storage when it doesn't exist.
  printf '%s\n' "$password" | NSSA_WALLET_HOME_DIR="$home_dir" wallet check-health >/dev/null
}

run_wallet() {
  local home_dir="$1"
  shift
  NSSA_WALLET_HOME_DIR="$home_dir" wallet "$@"
}

run_pd_bin() {
  local home_dir="$1"
  shift
  NSSA_WALLET_HOME_DIR="$home_dir" cargo run --manifest-path "$PROGRAM_DEPLOYMENT_DIR/Cargo.toml" --bin "$@"
}

echo "==> [1/4] Creating issuer wallet at $ISSUER_HOME"
init_wallet_home "$ISSUER_HOME" "$ISSUER_PASSWORD"

echo "==> Deploying airdrop programs (issuer)"
if ! run_wallet "$ISSUER_HOME" deploy-program "$BUILD_DIR/airdrop.bin"; then
  echo "warning: deploy airdrop.bin failed (possibly already deployed), continuing"
fi
if ! run_wallet "$ISSUER_HOME" deploy-program "$BUILD_DIR/airdrop_ticket_init.bin"; then
  echo "warning: deploy airdrop_ticket_init.bin failed (possibly already deployed), continuing"
fi

echo "==> [2/4] Generating ticket secrets and Merkle tree"
TICKETS_FILE="$TMP_DIR/tickets.txt"
: > "$TICKETS_FILE"
for _ in $(seq 0 $((NUM_RECIPIENTS - 1))); do
  openssl rand -hex 32 >> "$TICKETS_FILE"
done

MERKLE_OUT="$TMP_DIR/merkle.out"
NSSA_WALLET_HOME_DIR="$ISSUER_HOME" \
  cargo run --manifest-path "$PROGRAM_DEPLOYMENT_DIR/Cargo.toml" --bin gen_airdrop_merkle -- \
  --tickets-file "$TICKETS_FILE" | tee "$MERKLE_OUT"

MERKLE_ROOT="$(grep '^merkle_root=' "$MERKLE_OUT" | head -n1 | cut -d= -f2)"
if [[ -z "$MERKLE_ROOT" ]]; then
  echo "Failed to parse merkle_root from $MERKLE_OUT" >&2
  exit 1
fi
echo "Merkle root: $MERKLE_ROOT"

echo "==> Initializing airdrop registry with merkle root"
INIT_OUT="$TMP_DIR/init_airdrop.out"
NSSA_WALLET_HOME_DIR="$ISSUER_HOME" \
  cargo run --manifest-path "$PROGRAM_DEPLOYMENT_DIR/Cargo.toml" --bin init_airdrop -- \
  "$BUILD_DIR/airdrop.bin" "$MERKLE_ROOT" | tee "$INIT_OUT"

REGISTRY_ID="$(grep 'Created new Registry ID:' "$INIT_OUT" | tail -n1 | awk '{print $5}')"
if [[ -z "$REGISTRY_ID" ]]; then
  echo "Failed to parse Registry ID from init output" >&2
  exit 1
fi
echo "Registry ID: Public/$REGISTRY_ID"

echo "==> [3/4] Creating recipient wallets + private accounts with ticket secret"
mapfile -t TICKETS < "$TICKETS_FILE"
declare -a RECIPIENT_IDS

for i in $(seq 0 $((NUM_RECIPIENTS - 1))); do
  RECIPIENT_HOME="$RECIPIENTS_ROOT/r$i"
  RECIPIENT_PASSWORD="${RECIPIENT_PASSWORD_PREFIX}${i}"

  init_wallet_home "$RECIPIENT_HOME" "$RECIPIENT_PASSWORD"

  SECRET_HEX="${TICKETS[$i]}"
  INIT_TICKET_OUT="$TMP_DIR/init_ticket_${i}.out"
  NSSA_WALLET_HOME_DIR="$RECIPIENT_HOME" \
    cargo run --manifest-path "$PROGRAM_DEPLOYMENT_DIR/Cargo.toml" --bin init_airdrop_ticket -- \
    "$BUILD_DIR/airdrop_ticket_init.bin" "$SECRET_HEX" | tee "$INIT_TICKET_OUT"

  RECIPIENT_ID="$(grep 'Created new private account:' "$INIT_TICKET_OUT" | tail -n1 | awk '{print $5}')"
  if [[ -z "$RECIPIENT_ID" ]]; then
    echo "Failed to parse recipient private account ID for recipient $i" >&2
    exit 1
  fi
  RECIPIENT_IDS[$i]="$RECIPIENT_ID"

  run_wallet "$RECIPIENT_HOME" account sync-private >/dev/null
  echo "Recipient $i private account: Private/${RECIPIENT_IDS[$i]}"
done

echo "==> [4/4] Claiming airdrop from each recipient wallet"
for i in $(seq 0 $((NUM_RECIPIENTS - 1))); do
  RECIPIENT_HOME="$RECIPIENTS_ROOT/r$i"
  RECIPIENT_ID="${RECIPIENT_IDS[$i]}"
  LINE="$(grep "^ticket_index=${i} " "$MERKLE_OUT" || true)"
  if [[ -z "$LINE" ]]; then
    echo "Missing Merkle path for recipient index $i" >&2
    exit 1
  fi
  PATH_CSV="$(echo "$LINE" | sed -E 's/.* path_csv=//')"

  NSSA_WALLET_HOME_DIR="$RECIPIENT_HOME" \
    cargo run --manifest-path "$PROGRAM_DEPLOYMENT_DIR/Cargo.toml" --bin claim_airdrop -- \
    "$BUILD_DIR/airdrop.bin" \
    "Public/${REGISTRY_ID}" \
    "Private/${RECIPIENT_ID}" \
    "$PATH_CSV" \
    "$i" | tee "$TMP_DIR/claim_${i}.out"

  run_wallet "$RECIPIENT_HOME" account sync-private >/dev/null
  echo "Claim done for recipient $i (Private/${RECIPIENT_ID})"
done

echo "============================================="
echo "Airdrop test flow finished successfully."
echo "Test root: $TEST_ROOT"
echo "Registry: Public/$REGISTRY_ID"
for i in $(seq 0 $((NUM_RECIPIENTS - 1))); do
  echo "Recipient $i: NSSA_WALLET_HOME_DIR=$RECIPIENTS_ROOT/r$i account=Private/${RECIPIENT_IDS[$i]}"
done
