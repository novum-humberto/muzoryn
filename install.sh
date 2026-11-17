#!/usr/bin/env bash
# Instalador de MUZORYN: Debian/Ubuntu/Mint, Arch/Manjaro, Fedora/RHEL, openSUSE, Alpine, Void.
# Uso: ./install.sh [--user | --system] [--prefix DIR] [--no-deps] [--yes] [--help]
set -euo pipefail

PREFIX=""; MODE=""; DEPS=1; YES=0
SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MIN_RUST="1.82"

say()  { printf '\033[1;32m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!!\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31mError:\033[0m %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --user) MODE=user ;;
    --system) MODE=system ;;
    --prefix) PREFIX="${2:?falta DIR}"; shift ;;
    --no-deps) DEPS=0 ;;
    -y|--yes) YES=1 ;;
    -h|--help) sed -n '2,3p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "opción desconocida: $1" ;;
  esac; shift
done

[ -f "$SRC/Cargo.toml" ] || die "ejecuta install.sh dentro de la carpeta del proyecto (no se halló Cargo.toml)"
[ "$(uname -s)" = "Linux" ] || warn "sistema no Linux: la instalación de dependencias se omite"

if [ -z "$MODE" ]; then [ "$(id -u)" -eq 0 ] && MODE=system || MODE=user; fi
if [ -z "$PREFIX" ]; then [ "$MODE" = system ] && PREFIX=/usr/local || PREFIX="$HOME/.local"; fi

SUDO=""
if [ "$(id -u)" -ne 0 ] && { [ "$MODE" = system ] || [ "$DEPS" -eq 1 ]; }; then
  command -v sudo >/dev/null 2>&1 && SUDO=sudo || SUDO=""
fi
asroot() { if [ "$(id -u)" -eq 0 ]; then "$@"; elif [ -n "$SUDO" ]; then $SUDO "$@"; else die "se necesitan privilegios de root (instala sudo o ejecuta como root)"; fi; }

# ---- distro ----
ID_=""; LIKE_=""
[ -r /etc/os-release ] && . /etc/os-release && { ID_="${ID:-}"; LIKE_="${ID_LIKE:-}"; }
family() {
  case " $ID_ $LIKE_ " in
    *" debian "*|*" ubuntu "*) echo debian ;;
    *" arch "*) echo arch ;;
    *" fedora "*|*" rhel "*|*" centos "*) echo fedora ;;
    *" suse "*|*" opensuse "*) echo suse ;;
    *" alpine "*) echo alpine ;;
    *" void "*) echo void ;;
    *) command -v apt-get >/dev/null && echo debian || command -v pacman >/dev/null && echo arch || command -v dnf >/dev/null && echo fedora || echo unknown ;;
  esac
}
FAM="$(family)"; say "Distribución: ${PRETTY_NAME:-$ID_} (familia: $FAM)"

install_deps() {
  case "$FAM" in
    debian) asroot apt-get update -y
            asroot apt-get install -y build-essential pkg-config libasound2-dev curl ca-certificates cargo rustc ;;
    arch)   asroot pacman -S --needed --noconfirm base-devel alsa-lib pkgconf curl rust ;;
    fedora) asroot dnf install -y gcc make pkgconf-pkg-config alsa-lib-devel curl cargo rust ;;
    suse)   asroot zypper --non-interactive install gcc make pkg-config alsa-devel curl cargo rust ;;
    alpine) asroot apk add build-base pkgconf alsa-lib-dev curl cargo rust ;;
    void)   asroot xbps-install -Sy base-devel alsa-lib-devel pkg-config curl cargo rust ;;
    *) warn "distribución no reconocida: instala a mano un compilador C, pkg-config, ALSA (headers) y Rust >= $MIN_RUST" ;;
  esac
}

rust_ok() {
  command -v cargo >/dev/null 2>&1 || return 1
  local v; v="$(rustc --version 2>/dev/null | awk '{print $2}')"; [ -n "$v" ] || return 1
  [ "$(printf '%s\n%s\n' "$MIN_RUST" "$v" | sort -V | head -n1)" = "$MIN_RUST" ]
}

if [ "$DEPS" -eq 1 ] && [ "$(uname -s)" = "Linux" ]; then
  say "Instalando dependencias de compilación"
  install_deps || warn "no se pudieron instalar todas las dependencias; se intenta continuar"
fi

[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
if ! rust_ok; then
  warn "Rust >= $MIN_RUST no disponible; se instala con rustup (requiere red)"
  command -v curl >/dev/null || die "falta curl para obtener rustup"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
  . "$HOME/.cargo/env"
  rust_ok || die "no se pudo conseguir Rust >= $MIN_RUST"
fi

say "Compilando (puede tardar unos minutos)"
( cd "$SRC" && cargo build --release --locked 2>/dev/null || cargo build --release )
BIN="$SRC/target/release/muzoryn"; [ -x "$BIN" ] || die "no se generó el binario"

say "Instalando en $PREFIX"
put() { if [ "$MODE" = system ]; then asroot "$@"; else "$@"; fi; }
put install -d "$PREFIX/bin" "$PREFIX/share/muzoryn/themes" "$PREFIX/share/doc/muzoryn" "$PREFIX/share/icons/hicolor/scalable/apps" "$PREFIX/share/icons/hicolor/512x512/apps" "$PREFIX/share/applications"
put install -m 0755 "$BIN" "$PREFIX/bin/muzoryn"
put install -m 0644 "$SRC"/themes/*.toml "$PREFIX/share/muzoryn/themes/"
[ -f "$SRC/config/keys.toml" ] && put install -m 0644 "$SRC/config/keys.toml" "$PREFIX/share/muzoryn/keys.toml"
[ -f "$SRC/README.md" ] && put install -m 0644 "$SRC/README.md" "$PREFIX/share/doc/muzoryn/README.md"
[ -f "$SRC/icons/muzoryn.svg" ] && put install -m 0644 "$SRC/icons/muzoryn.svg" "$PREFIX/share/icons/hicolor/scalable/apps/muzoryn.svg"
[ -f "$SRC/icons/muzoryn-option1.png" ] && put install -m 0644 "$SRC/icons/muzoryn-option1.png" "$PREFIX/share/icons/hicolor/512x512/apps/muzoryn.png"
[ -f "$SRC/muzoryn.desktop" ] && put install -m 0644 "$SRC/muzoryn.desktop" "$PREFIX/share/applications/muzoryn.desktop"
# Registro para uninstall.sh
put sh -c "printf '%s\n' '$PREFIX' > '$PREFIX/share/muzoryn/.prefix'"

case ":$PATH:" in *":$PREFIX/bin:"*) ;; *) warn "añade $PREFIX/bin a tu PATH (p. ej. export PATH=\"$PREFIX/bin:\$PATH\")" ;; esac
say "Listo: $("$PREFIX/bin/muzoryn" --version)"
echo "Manual: muzoryn --man | ayuda: muzoryn --help"
echo "Icono: instalado en hicolor (SVG y PNG 512x512, menú de aplicaciones)."
echo "Gráficos: kitty/ghostty/WezTerm (kitty), foot/mlterm/Konsole (sixel), iTerm2. MUZORYN_GRAPHICS=kitty|iterm|sixel|blocks lo fuerza."
echo "Se recomienda una fuente Nerd Font para los iconos."
