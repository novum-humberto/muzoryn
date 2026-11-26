# ⚡ Instalación

MUZORYN se compila desde el código fuente. El instalador automático lo hace por ti en Linux.

**Requisitos:** Rust estable ≥ 1.82, un compilador de C (SQLite va incluido), y en Linux `libasound2-dev` (o su equivalente) y `pkg-config`.

## Opción A · Instalador automático (recomendado)

```bash
git clone https://github.com/TU-USUARIO/muzoryn.git
cd muzoryn
chmod +x install.sh uninstall.sh   # solo si hace falta
./install.sh
```

Detecta tu distribución (Debian/Ubuntu, Arch, Fedora, openSUSE, Alpine, Void), instala las dependencias, compila y copia el programa a `~/.local`.

| Opción | Qué hace |
|---|---|
| `--user` | Instala solo para tu usuario en `~/.local` (por defecto si no eres root) |
| `--system` | Instala para todos en `/usr/local` (usa `sudo`) |
| `--prefix DIR` | Instala en otra carpeta |
| `--no-deps` | No instala dependencias |
| `-y`, `--yes` | Modo no interactivo; también confirma la descarga verificada de rustup |
| `-h`, `--help` | Ayuda |

### Si falta Rust
Si no hay Rust ≥ 1.82, el instalador:
1. te explica lo que va a hacer y **pide confirmación** (sin terminal interactiva se niega, salvo con `--yes`);
2. descarga `rustup-init` y su suma `.sha256` por HTTPS (TLS 1.2);
3. **compara el SHA-256** y solo entonces lo ejecuta (perfil mínimo, en `~/.cargo` y `~/.rustup`).

No usa `curl | sh`. Si prefieres hacerlo a mano, instala Rust desde <https://rustup.rs> y usa `./install.sh --no-deps`.

## Opción B · Dependencias por distribución

| Sistema | Comando |
|---|---|
| Debian / Ubuntu / Mint | `sudo apt install -y build-essential pkg-config libasound2-dev curl ca-certificates cargo rustc` |
| Arch / Manjaro / Omarchy | `sudo pacman -S --needed base-devel alsa-lib pkgconf curl rust` |
| Fedora / RHEL | `sudo dnf install -y gcc make pkgconf-pkg-config alsa-lib-devel curl cargo rust` |
| openSUSE | `sudo zypper install gcc make pkg-config alsa-devel curl cargo rust` |
| Alpine | `sudo apk add build-base pkgconf alsa-lib-dev curl cargo rust` |
| Void | `sudo xbps-install -Sy base-devel alsa-lib-devel pkg-config curl cargo rust` |

Después sigue con la Opción C.

## Opción C · Manual con Cargo

```bash
cargo build --release
./target/release/muzoryn
```

Para instalarlo sin el script:

```bash
install -Dm755 target/release/muzoryn ~/.local/bin/muzoryn
mkdir -p ~/.local/share/muzoryn/themes && cp themes/*.toml ~/.local/share/muzoryn/themes/
```

## Comprobar

```bash
muzoryn --version     # muzoryn 7.0.1
```

Si el comando no se encuentra, añade `~/.local/bin` al `PATH` (ver [solución de problemas](solucion-de-problemas.md)).

## Actualizar

Si ya tienes MUZORYN instalado y quieres pasar a una versión nueva, **tus ajustes, biblioteca, listas, podcasts y favoritas se conservan** con los dos métodos (viven en `~/.config/muzoryn` y `~/.local/share/muzoryn`). Las opciones nuevas toman su valor por defecto.

> ⚠️ **No uses `--purge` para actualizar:** borra configuración, biblioteca, listas y podcasts.

### 1 · Consigue la versión nueva
- Si clonaste el repositorio: `cd muzoryn && git pull`.
- Si descargaste el ZIP: **Code → Download ZIP** (o la *release*), descomprímelo en una carpeta nueva y entra en ella.

### 2 · Método A · Desinstalar y reinstalar (recomendado)
```bash
muzoryn ctl quit        # cierra el daemon antiguo (el desinstalador también lo intenta)
./uninstall.sh          # SIN --purge
./install.sh            # con las mismas opciones que usaste la primera vez
muzoryn --version       # muzoryn 7.0.1
```
Es el método más limpio: elimina el programa anterior (binario, iconos, documentación y archivos de la aplicación) antes de instalar el nuevo, y evita que queden dos copias si cambiaste de ubicación.

### 2 · Método B · Instalar encima
```bash
muzoryn ctl quit
./install.sh            # con las MISMAS opciones que usaste la primera vez
muzoryn --version
```
Sobrescribe el binario, los temas, los iconos, el acceso del menú y la documentación instalados. Es más rápido, pero pueden quedar archivos que la versión nueva ya no usa. **Repite las mismas opciones** (`--system`, `--prefix DIR`…): si cambias de ubicación, la copia antigua seguirá instalada en la otra. Comprueba con `which -a muzoryn`, que debe mostrar una sola ruta.

### 3 · Comprueba
- `muzoryn --version` muestra la versión nueva.
- Al abrir `muzoryn` ves tu biblioteca, emisoras y podcasts.
- Si no necesitas el daemon antiguo y la interfaz muestra la cola vacía, ejecuta `muzoryn ctl quit` y abre `muzoryn` otra vez.
- Hooks: desde la 7.0.1 solo se activan con `settings.toml` y carpeta no escribibles por otros ([Hooks](hooks.md)).

## Desinstalar

```bash
./uninstall.sh            # borra el programa; conserva ajustes y biblioteca
./uninstall.sh --purge    # borra también configuración, biblioteca, listas y podcasts
```

## Otros sistemas
El instalador está pensado para **Linux**. En Windows usa **WSL2** (el audio depende de tu configuración). En macOS no se ha probado.
