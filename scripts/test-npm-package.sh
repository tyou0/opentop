#!/bin/sh
set -eu

repo_dir=$(CDPATH= cd -P -- "$(dirname -- "$0")/.." && pwd)
test_dir=$(mktemp -d)
trap 'rm -rf -- "$test_dir"' EXIT HUP INT TERM
cd "$repo_dir"

check_tui() {
  smoke=$(python3 scripts/tui_perf.py --binary "$1" --scenario smoke --cols 200 --duration 0.1)
  if ! printf '%s\n' "$smoke" | grep -Eq '^first_output_ms=[0-9]' ||
     ! printf '%s\n' "$smoke" | grep -Eq '^quit_ms=[0-9]'; then
    printf '%s\n' "$smoke" >&2
    return 1
  fi
}

npm pack --ignore-scripts --pack-destination "$test_dir" >/dev/null
archive=$test_dir/opentop-rs-$(node -p 'require("./package.json").version').tgz
npm install -g --ignore-scripts --prefix "$test_dir/npm prefix" "$archive" >/dev/null
npm_bin="$test_dir/npm prefix/bin"
"$npm_bin/opentop-cli" --version | grep -q '^opentop-cli '
"$npm_bin/opentop-cli" dump --json | python3 -c 'import json, sys; data = json.load(sys.stdin); assert all(key in data for key in ("cpu", "memory", "disks", "processes"))'
check_tui "$npm_bin/opentop"

if command -v bun >/dev/null 2>&1; then
  bun_path=$(command -v bun)
  mkdir -p "$test_dir/no-node"
  for tool in basename dirname readlink uname; do
    ln -s "$(command -v "$tool")" "$test_dir/no-node/$tool"
  done
  PATH="$test_dir/no-node" BUN_INSTALL="$test_dir/bun" "$bun_path" install -g "$archive" >/dev/null
  bun_bin=$test_dir/bun/bin
  PATH="$test_dir/no-node" "$bun_bin/opentop-cli" --version | grep -q '^opentop-cli '
  if PATH="$test_dir/no-node" "$bun_bin/opentop" --invalid-option 2>"$test_dir/error"; then
    echo 'Expected the native app to reject an invalid option' >&2
    exit 1
  fi
  grep -q 'unknown argument' "$test_dir/error"
  check_tui "$bun_bin/opentop"
fi

mkdir -p "$test_dir/unsupported"
printf '#!/bin/sh\nprintf "UnknownOS\\n"\n' > "$test_dir/unsupported/uname"
chmod +x "$test_dir/unsupported/uname"
if PATH="$test_dir/unsupported:$PATH" "$npm_bin/opentop" 2>"$test_dir/error"; then
  echo 'Expected unsupported platform to fail' >&2
  exit 1
fi
grep -q 'unsupported platform' "$test_dir/error"

native_dir="$test_dir/npm prefix/lib/node_modules/opentop-rs/npm/native"
mv "$native_dir" "$native_dir-missing"
if "$npm_bin/opentop" 2>"$test_dir/error"; then
  echo 'Expected a missing binary to fail' >&2
  exit 1
fi
grep -q 'prebuilt binary missing' "$test_dir/error"
echo 'npm package checks passed'
