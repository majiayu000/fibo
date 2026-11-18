#!/bin/sh
# Fast structural checks before pushing; full runtime/UI checks stay in CI.
set -eu
cd "$(dirname "$0")/.."
cargo fmt --check
cargo test -p fibo-core --test guard --test file_size --test i18n
cargo test -p fibo-platform --test boundary
cargo test -p fibo-host --test architecture_boundary
cargo test -p fibo-host --lib cli_setup::tests
