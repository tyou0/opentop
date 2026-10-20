#!/usr/bin/env bash
set -e

# opentop installation script
# Builds and optionally installs opentop to system

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_DIR="/usr/local/bin"
PREFIX="$INSTALL_DIR"

print_usage() {
  cat << USAGE
Usage: $0 [OPTIONS]

Options:
  -h, --help           Show this help message
  --prefix <dir>       Installation prefix (default: /usr/local)
  --no-install         Build only, don't install
  --uninstall          Uninstall opentop from system
USAGE
}

NO_INSTALL=false
UNINSTALL=false

while [[ $# -gt 0 ]]; do
  case $1 in
    -h|--help)
      print_usage
      exit 0
      ;;
    --prefix)
      PREFIX="$2/bin"
      shift 2
      ;;
    --no-install)
      NO_INSTALL=true
      shift
      ;;
    --uninstall)
      UNINSTALL=true
      shift
      ;;
    *)
      echo "Unknown option: $1"
      print_usage
      exit 1
      ;;
  esac
done

if [ "$UNINSTALL" = true ]; then
  echo "Uninstalling opentop..."
  rm -f "$INSTALL_DIR/opentop" "$INSTALL_DIR/opentop-cli" 2>/dev/null || true
  rm -f "$PREFIX/opentop" "$PREFIX/opentop-cli" 2>/dev/null || true
  echo "Uninstalled."
  exit 0
fi

echo "Building opentop..."
cd "$REPO_DIR"
cargo build --release

if [ "$NO_INSTALL" = true ]; then
  echo "Build complete. Binaries:"
  echo "  $REPO_DIR/target/release/opentop"
  echo "  $REPO_DIR/target/release/opentop-cli"
  exit 0
fi

echo "Installing to $PREFIX..."
mkdir -p "$PREFIX"
cp "$REPO_DIR/target/release/opentop" "$PREFIX/"
cp "$REPO_DIR/target/release/opentop-cli" "$PREFIX/"
chmod +x "$PREFIX/opentop" "$PREFIX/opentop-cli"

echo "Installation complete!"
echo "  $PREFIX/opentop"
echo "  $PREFIX/opentop-cli"
