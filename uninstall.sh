#!/usr/bin/env bash
# Desinstalador de MUZORYN. Uso: ./uninstall.sh [--prefix DIR] [--purge] [--yes]
#   --purge  borra también configuración (~/.config/muzoryn) y datos (~/.local/share/muzoryn)
set -euo pipefail
PREFIX=""; PURGE=0; YES=0
die()  { printf '\033[1;31mError:\033[0m %s\n' "$*" >&2; exit 1; }
say()  { printf '\033[1;32m==>\033[0m %s\n' "$*"; }
while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) PREFIX="${2:?falta DIR}"; shift ;;
    --purge) PURGE=1 ;;
    -y|--yes) YES=1 ;;
    -h|--help) sed -n '2,3p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "opción desconocida: $1" ;;
  esac; shift
done

# Prefijos candidatos si no se indica uno
CANDIDATES=()
if [ -n "$PREFIX" ]; then CANDIDATES=("$PREFIX"); else CANDIDATES=("$HOME/.local" /usr/local /usr); fi

# Detiene el daemon si está activo
if command -v muzoryn >/dev/null 2>&1; then muzoryn ctl quit >/dev/null 2>&1 || true; fi

FOUND=0
for P in "${CANDIDATES[@]}"; do
  [ -e "$P/bin/muzoryn" ] || [ -d "$P/share/muzoryn" ] || continue
  FOUND=1
  run() { if [ -w "$P" ] || [ "$(id -u)" -eq 0 ]; then "$@"; elif command -v sudo >/dev/null; then sudo "$@"; else die "se necesita root para borrar en $P"; fi; }
  say "Eliminando de $P"
  run rm -f "$P/bin/muzoryn"

  # En una instalación --user, PREFIX suele ser ~/.local y
  # PREFIX/share/muzoryn coincide exactamente con XDG_DATA_HOME/muzoryn.
  # Esa carpeta contiene biblioteca, podcasts, listas y otros datos del usuario:
  # nunca se borra durante una desinstalación normal.
  APP_DATA="$P/share/muzoryn"
  USER_DATA="${XDG_DATA_HOME:-$HOME/.local/share}/muzoryn"
  APP_DATA_REAL="$(realpath -m "$APP_DATA")"
  USER_DATA_REAL="$(realpath -m "$USER_DATA")"
  if [ "$APP_DATA_REAL" != "$USER_DATA_REAL" ]; then
    run rm -rf "$APP_DATA"
  else
    say "Conservando datos del usuario en $USER_DATA"
  fi

  run rm -rf "$P/share/doc/muzoryn"
  run rm -f "$P/share/icons/hicolor/scalable/apps/muzoryn.svg" "$P/share/icons/hicolor/512x512/apps/muzoryn.png" "$P/share/applications/muzoryn.desktop"
done
[ "$FOUND" -eq 1 ] || echo "No se encontró ninguna instalación de MUZORYN en: ${CANDIDATES[*]}"

rm -f "${XDG_RUNTIME_DIR:-/tmp}/muzoryn.sock" "/tmp/muzoryn-${USER:-user}.sock" 2>/dev/null || true

if [ "$PURGE" -eq 1 ]; then
  CFG="${XDG_CONFIG_HOME:-$HOME/.config}/muzoryn"; DAT="${XDG_DATA_HOME:-$HOME/.local/share}/muzoryn"
  if [ "$YES" -eq 0 ]; then
    read -r -p "¿Borrar $CFG y $DAT (biblioteca, listas, podcasts)? [s/N] " a
    case "$a" in s|S|y|Y) ;; *) echo "Datos conservados."; exit 0 ;; esac
  fi
  rm -rf "$CFG" "$DAT"; say "Configuración y datos eliminados"
else
  echo "Configuración y datos conservados (usa --purge para borrarlos)."
fi
say "MUZORYN desinstalado"
