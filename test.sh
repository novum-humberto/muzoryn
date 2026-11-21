#!/usr/bin/env bash
# Verificación de desarrollo de MUZORYN. No modifica el formato del código.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT" || exit 2

run() {
  printf '\n==> %s\n' "$1"
  shift
  "$@" || return $?
}

command -v cargo >/dev/null 2>&1 || { echo 'Error: instala Rust y Cargo antes de ejecutar las pruebas.' >&2; exit 127; }
run 'Formato' cargo fmt --all -- --check || exit $?
run 'Pruebas (incluye MPRIS por defecto)' cargo test --all-targets || exit $?
run 'Clippy' cargo clippy --all-targets -- -D warnings || exit $?

echo '\nTodas las comprobaciones finalizaron correctamente.'
