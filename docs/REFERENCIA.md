[← Volver al README](../README.md)

# MUZORYN v6.9.1-Pro

**Tu música. Tu control. En la terminal.** Reproductor musical en terminal escrito en Rust
(Ratatui + Crossterm + Rodio + SQLite). La interfaz reproduce las 12 secciones de la
imagen de referencia (`interfaz.png`).

> **Nota de versión 0.6.0:** el proyecto pasó de llamarse *Sonora* a **MUZORYN**. Cambian el
> binario (`muzoryn`), las carpetas (`~/.config/muzoryn`, `~/.local/share/muzoryn`) y las
> variables (`MUZORYN_*`). Para conservar tus ajustes copia `~/.config/sonora/settings.toml`
> a `~/.config/muzoryn/`. `interfaz.png` aún muestra el nombre antiguo.

> **Instalación:** `./install.sh` (Debian/Ubuntu, Arch, Fedora, openSUSE, Alpine, Void) · `./uninstall.sh [--purge]`.
> **6.9.1-Pro:** `D` en Podcasts descarga **el canal completo** (todos los episodios que falten); `d` sigue
> descargando un solo episodio.
>
> **6.9.0-Pro:** **explorador de podcasts en español** en Podcasts (8): búsqueda, más escuchados y 19
> categorías, con suscripción. Detalle en «Podcasts en español».
>
> **6.8.2-Pro:** los filtros de Radio Browser se escriben en español (ver «Radio Browser»).
>
> **6.8.1-Pro:** los flujos de radio y podcasts se abren **en segundo plano** (la interfaz y el
> teclado no se bloquean con mala red; `s` cancela) y se leen con un **búfer de ~1 MiB**.
>
> **6.8.0-Pro:** Radio con **favoritas ★**, **historial reciente**, **canción en vivo (ICY)**, **reconexión automática**
> y estado de conexión (*Conectando… / En vivo / Reconectando… / Emisora no disponible*). Detalle en «Radio» más abajo.
>
> **6.7.0-Pro:** incorpora el submenú Radio Browser en Radio. Busca por nombre o mediante `country:`, `language:` y `genre:`; Enter reproduce y añade la emisora a la lista local.
>
> **0.6.6+:** Sixel (foot/mlterm) ya coloca la carátula en su sitio; iconos por tipo de archivo; importación TOML tolerante.
>
> **0.6.6:** carátulas reales (etiqueta embebida, `cover/folder/front.jpg` o imagen del feed del podcast) en reproducción, Álbumes y Podcasts; carátula de radio por defecto; iconos de carpeta Nerd Font. Requiere una fuente Nerd Font.
> **Carátulas nítidas:** en terminales con gráficos (Kitty, WezTerm, Ghostty → protocolo Kitty; iTerm2/mintty → iTerm2; foot, mlterm, Konsole → Sixel) las carátulas se muestran en resolución real (hasta 512 px). Resto: medios bloques a 96 px. Fuerza el modo con `MUZORYN_GRAPHICS=kitty|iterm|sixel|blocks`. Dentro de tmux se usa el respaldo de bloques.
> **0.6.2:** la Biblioteca navega por carpetas a cualquier profundidad (p. ej. `Jazz/Nina Simone/<álbum>`),
> con contador de pistas por carpeta, y permite componer la cola con carpetas o temas sueltos de distintas ramas.

> **0.6.3:** las listas se importan y exportan en **M3U, M3U8, TOML, XSPF y HTML**. En Listas (6),
> `i` abre un navegador flotante para localizar el archivo (el formato se detecta por la extensión) y `Ctrl+E` exporta: escribe el formato o deja
> vacío para generar los cinco en `~/.local/share/muzoryn/exports/`. XSPF es el formato XML estándar
> (VLC, Clementine…); el HTML se puede abrir en el navegador y también se reimporta.

## Emisoras y podcasts desde la interfaz

En **Radio (7)** o **Podcasts (8)** pulsa `a` (o `Ctrl+A`), pega la URL y pulsa Enter.
Después se pide un nombre: escribe uno personalizado o deja vacío para el automático
(emisora: nombre que anuncia el stream, o el dominio; podcast: título del feed, o el dominio).
`x` dos veces quita el elemento seleccionado. Todo se guarda en `settings.toml`.
Editar `[[stations]]` / `[[podcasts]]` a mano sigue funcionando: al entrar en Radio o
Podcasts, o al añadir uno, se incorporan las entradas nuevas del fichero.

## Estado de verificación (léelo primero)

Desde la 6.8.0-Pro el proyecto se compila y se prueba con `cargo test` (Rust ≥ 1.82; 103 pruebas
pasan: núcleo, ICY, favoritas/recientes, reconexión e interfaz con `TestBackend`).

Lo que **no** está comprobado en un equipo real: el audio (no hay tarjeta de sonido en el entorno
de pruebas), la lectura de metadatos ICY contra emisoras reales y el aviso de clics a Radio Browser.
Si una emisora muestra ruido o no muestra canción, repórtala con su URL.

## Compilar y ejecutar

Requisitos: Rust estable ≥ 1.82, un compilador de C (rusqlite compila SQLite incluido) y,
en Linux, `libasound2-dev` y `pkg-config` (backend de audio de Rodio).

```sh
cargo build --release
cargo test
./target/release/muzoryn                       # abre la interfaz
./target/release/muzoryn ~/Music/Album         # añade archivos o carpetas a la cola y reproduce
```

La primera vez crea `~/.config/muzoryn/settings.toml` y escanea `~/Music` (cámbialo en
`music_dirs`). La biblioteca se guarda en `~/.local/share/muzoryn/library.db`.

## Las 12 vistas

| Tecla | Vista | Qué hace |
|---|---|---|
| `1` | Inicio | Portada generada, espectro en vivo, controles y cola |
| `2` | Biblioteca | Árbol de carpetas desplegable (`→` despliega, `←` pliega/sube, `Enter` alterna) + tabla de canciones. `a` en el árbol añade la carpeta entera (con subcarpetas) a la cola; `a` en la tabla, una canción |
| `3` | Búsqueda | Búsqueda global instantánea (canciones, álbumes, artistas) |
| `4` | Cola | Reordenar (Alt+↑↓), quitar, vaciar |
| `5` | Álbumes y artistas | Cuadrícula con portadas; pestañas Álbumes/Artistas/Géneros |
| `6` | Listas | Favoritas y listas propias; importar/exportar playlists |
| `7` | Radio | Emisoras HTTP en directo (`a` añade una por URL, `x` quita) |
| `8` | Podcasts | Feeds RSS (`a` añade por URL, `x` quita), streaming y descarga; `/` busca podcasts en español y `Tab` abre Populares y Categorías |
| `9` | Letras | Letras sincronizadas desde archivos `.lrc`, con desfase ±0,5 s |
| `0` | Audio y EQ | Ecualizador de 10 bandas, presets, ReplayGain, crossfade |
| `v` | Vista completa | Portada grande y espectro a pantalla completa |
| `,` | Ajustes | General, Audio, Biblioteca, Daemon/IPC, Avanzado, Acerca de y Temas |

`muzoryn --help` o `muzoryn -h` muestra la ayuda breve; `muzoryn --man` imprime este manual completo.

## Armar una lista variada (selección múltiple)

Puedes juntar pistas de distintos lugares y añadirlas de una vez a una lista:

1. En **Listas (6)** elige la lista destino.
2. Ve marcando con `m` (vuelve a pulsar `m` para desmarcar):
   - en **Biblioteca (2)**: una pista, o una carpeta entera si el foco está en el panel de carpetas;
   - en **Álbumes (5)**: un álbum, artista o género completo;
   - en **Búsqueda (3)**: pistas, álbumes o artistas;
   - en **Cola (4)**: la pista seleccionada.
3. Las marcadas muestran `+` y el encabezado indica cuántas llevas. Las marcas se conservan al cambiar de vista.
4. Pulsa `l` para añadirlas todas a la lista. `M` quita todas las marcas. Sin marcas, `l` añade solo lo seleccionado.

## Configuración recomendada de la terminal

La terminal dibuja una cuadrícula de celdas y no deja pintar su padding interno. MUZORYN pone el fondo
de la terminal al color del tema (secuencia OSC 11) y lo restaura al salir; para desactivarlo usa
`MUZORYN_NO_OSC=1 muzoryn`. Para que la interfaz ocupe todo el espacio, quita el padding:

| Terminal | Archivo | Opciones |
|---|---|---|
| Kitty | `kitty.conf` | `window_padding_width 0` y `placement_strategy top-left` |
| Alacritty | `alacritty.toml` | `[window]` → `padding = { x = 0, y = 0 }` y `dynamic_padding = false` |
| foot | `foot.ini` | `[main]` → `pad=0x0` |
| Ghostty | `config` | `window-padding-x = 0`, `window-padding-y = 0`, `window-padding-color = extend` |
| WezTerm | `wezterm.lua` | `config.window_padding = { left = 0, right = 0, top = 0, bottom = 0 }` |

Si ves un borde de color alrededor, puede venir del gestor de ventanas (por ejemplo `col.active_border`
en Hyprland) o de la transparencia de la terminal, no de MUZORYN. Fuente recomendada: una Nerd Font,
para los iconos de carpeta.

## Atajos principales

`espacio` pausa · `n`/`p` siguiente/anterior · `←`/`→` buscar ∓5 s · `=`/`-` volumen ·
`z` aleatorio · `o` repetir (no/todo/una) · `c` crossfade (0/3/6/9 s) · `g` ReplayGain ·
`e` preset del EQ · `t` EQ on/off · `/` buscar · `a` añadir a la cola · `f` favorito ·
`m` marcar · `M` quitar marcas · `l` añadir (marcadas o actual) a la lista elegida · `u` actualizar biblioteca/feed · `d` descargar episodio · `D` descargar el canal completo ·
`Ctrl+N` nueva lista · `F2` renombrar · `i` importar M3U · `Ctrl+E` exportar M3U · `x` quitar ·
`Tab` cambiar panel/pestaña · `?` abre el manual integrado con navegación por páginas.

## Daemon (comportamiento tipo MOC)

Al abrir `muzoryn` el daemon se inicia solo en segundo plano. `w` cierra la interfaz y
el audio sigue sonando; al volver a abrir, la interfaz aparece en la misma vista y panel
(Biblioteca, Radio, etc.). `W` (mayúscula) o `Ctrl+C` cierra la interfaz y detiene el
daemon. También: `muzoryn ctl quit`.

En las vistas Álbumes y Audio, `←`/`→` mueven la selección en lugar de buscar.
Todos los atajos se pueden cambiar en `~/.config/muzoryn/keys.toml`
(ver `config/keys.toml`); un atajo propio reemplaza los de esa acción.

## Funciones del reproductor

- **Biblioteca SQLite** con escaneo incremental por fecha de modificación y etiquetas
  (título, artista, álbum, género, número de pista, duración y ReplayGain) vía `lofty`.
  Formatos: mp3, flac, ogg/opus, wav, m4a/aac, aiff.
- **Ecualizador de 10 bandas** (filtros biquad, ±12 dB) y **ReplayGain** por pista,
  aplicados en el flujo de audio, más **crossfade** con dos salidas solapadas.
- **Espectro en vivo** calculado de las muestras reales que se reproducen.
- **Aleatorio, repetición, cola editable**, volumen y búsqueda dentro de la pista.
- **Radio y podcasts** por HTTP. Las emisoras en directo no se pueden buscar. Radio incluye
  favoritas, historial, canción en vivo y reconexión (ver «Radio» más abajo).
- **Letras `.lrc`**: coloca `cancion.lrc` junto a `cancion.flac`.
- **Daemon + IPC**: `muzoryn --daemon` mantiene la reproducción al cerrar la interfaz;
  al abrir `muzoryn` se conecta solo. Control externo: `muzoryn ctl toggle|next|prev|stop|vol+|vol-|shuffle|repeat|forward|back|status`.
- **Plugins por hooks**: en `settings.toml`, `[[hooks]] event = "track_changed"`
  con un `command` de shell (variables `MUZORYN_TITLE`, `MUZORYN_ARTIST`, `MUZORYN_ALBUM`).
  Eventos: `track_changed`, `playback_stopped`.
- **Temas** TOML (`nocturne`, `daylight` o un archivo propio): `MUZORYN_THEME=daylight muzoryn`.

## Ejemplo de settings.toml

(Desde la 6.8.0-Pro cada `[[stations]]` admite además `fav`, `uuid` y `tags`; se rellenan solos.)

```toml
theme = "nocturne"
music_dirs = ["/home/yo/Music"]

[[stations]]
name = "Mi radio"
url = "https://ejemplo.org/stream.mp3"
bitrate = 128

[[podcasts]]
name = "Mi podcast"
url = "https://ejemplo.org/feed.xml"

[[hooks]]
event = "track_changed"
command = "notify-send \"$MUZORYN_TITLE\" \"$MUZORYN_ARTIST\""
```

## Límites conocidos

- Las URLs de emisoras y feeds por defecto (SomaFM, Radio Paradise, NPR) son ejemplos:
  sustitúyelas por las tuyas.
- Los streams de red se abren en un hilo aparte (hasta ~8 s de conexión y ~20 s por lectura
  antes de rendirse; mientras tanto se ve «Conectando…» y se puede parar o elegir otra
  emisora) y no admiten seek. El búfer de lectura (~1 MiB) absorbe microcortes, no cortes largos.
- La radio y los podcasts dependen de que el formato del flujo lo decodifique Symphonia.
- Un daemon remoto envía la cola completa en cada actualización; con colas de miles de
  pistas conviene usar la interfaz integrada.
- No hay MPRIS ni gapless real (solo crossfade). Las teclas multimedia se enlazan con
  `muzoryn ctl` (ver «Teclas multimedia»).

## Radio (6.8.0-Pro)

**Secciones.** En Radio, **Tab** recorre: *Mis emisoras → Favoritas ★ → Recientes → Explorar Radio Browser*.

- **Favoritas:** `f` marca o desmarca la emisora seleccionada (también un resultado de Radio Browser,
  que además se añade a Mis emisoras). Desde cualquier otra vista, `f` sin pista seleccionada marca la
  emisora que está sonando. Se guarda en `settings.toml` (`fav = true`).
- **Recientes:** las últimas 10 emisoras que reproduces (`recent_stations`, la más reciente primero).
  En esa sección `x` solo la quita del historial; en las demás, `x` (dos veces) elimina la emisora.
- **Canción en vivo:** si la emisora envía metadatos ICY, el reproductor muestra *artista – canción* y,
  debajo, el nombre de la emisora. Si no los envía, muestra la emisora y su género (`tags`, tomado de
  Radio Browser o de la cabecera `icy-genre`; «Radio en directo» si no hay ninguno).
- **Estado de la conexión** (barra del reproductor y panel «En el aire»): *Conectando…*, *En vivo*,
  *Reconectando…* y *Emisora no disponible*. Si el flujo se corta, se reintenta solo con esperas
  de 0, 2, 5, 10, 20 y 30 s sin saltar a otra pista; si todos fallan, se detiene y avisa.
- **Conexión lenta:** abrir una emisora no bloquea la interfaz; si falla, sigue sonando lo que
  había antes y se muestra el error. El audio se lee con ~1 MiB de antelación para tolerar bajones.
- **Clics a Radio Browser:** al reproducir una emisora que vino de Radio Browser se envía a su API
  únicamente el identificador de la emisora (`stationuuid`), para contar su popularidad. Se desactiva
  con `radio_click_count = false` en `settings.toml`. Las emisoras añadidas por URL no envían nada.

## Podcasts en español (6.9.0-Pro)

En **Podcasts (8)** puedes descubrir y suscribirte a podcasts sin conocer su URL. Usa el directorio público de
Apple Podcasts (sin cuenta ni clave) consultado con términos y categorías en español y contra una tienda
hispanohablante, de modo que salen podcasts **en español**, no solo la interfaz traducida.

| Tecla | Acción |
|---|---|
| `/` | Buscar por nombre o tema (p. ej. `historia de México`). Muestra hasta 50 resultados |
| `Tab` (desde Episodios) | Abre **Populares** → **Categorías** → vuelve a **Mis podcasts** |
| `Enter` | En una categoría la abre; en un podcast lo **suscribe y abre sus episodios** |
| `f` | Suscribirse o cancelar la suscripción sin abrirlo (✓ marca los suscritos) |
| `Esc` | Sale de una categoría; otra vez, sale del explorador |
| `u` | En **Mis podcasts**, actualiza el feed del canal seleccionado; en **Populares**, recarga el catálogo |

**Descargas.** En Mis podcasts, `d` descarga el episodio seleccionado y `D` **todo el canal**: pide confirmar
(pulsa `D` otra vez en 8 s, con el número de episodios nuevos), baja los que falten uno tras otro y muestra el
avance en el título del panel (`⇩ canal 12/80`). Si uno falla sigue con el siguiente; al terminar resume cuántos
bajaron y cuántos fallaron, y repetir `D` continúa solo con lo pendiente (útil con mala conexión). Pulsar `D`
durante la descarga la cancela. Se guardan en `~/.local/share/muzoryn/podcasts/`.

Las 19 categorías están en español: Noticias, Tecnología, Crimen real, Comedia, Sociedad y cultura, Historia,
Ciencia, Salud y bienestar, Negocios, Música, Deportes, Arte, Educación, Ficción, TV y cine, Religión y
espiritualidad, Niños y familia, Ocio y Gobierno.

**Tienda (país).** Se elige con `podcast_country` en `settings.toml` (código de 2 letras; por defecto `mx`).
Cambia los populares y el catálogo que se consulta: `es` (España), `ar`, `co`, `cl`, `pe`, `us`… Las
suscripciones se guardan como `[[podcasts]]`, igual que los feeds añadidos por URL.

**Límites.** Apple no indica el idioma de cada podcast, así que el filtro es la tienda más el idioma de la
consulta: casi todo sale en español, pero puede colarse alguno en otro idioma. Si el directorio no responde
(sin red, o bloqueado en tu país), la lista muestra el error y se puede seguir añadiendo feeds con `a`.

## Teclas multimedia

MUZORYN no usa MPRIS; enlaza las teclas de tu escritorio con el control externo. Ejemplo para Hyprland/Omarchy:

```
bindl = , XF86AudioPlay, exec, muzoryn ctl toggle
bindl = , XF86AudioNext, exec, muzoryn ctl next
bindl = , XF86AudioPrev, exec, muzoryn ctl prev
bindl = , XF86AudioStop, exec, muzoryn ctl stop
```

En sway/i3: `bindsym XF86AudioPlay exec muzoryn ctl toggle`.

## Estructura

```
src/core.rs     cola, estado, comandos, aleatorio/repetición, crossfade, servicio
src/audio.rs    Rodio + cadena DSP (EQ, ganancia, espectro) + motor nulo
src/dsp.rs      biquad, presets y espectro (probado sin dependencias)
src/library.rs  SQLite, escaneo, favoritos, listas, M3U
src/lyrics.rs   parser LRC
src/feeds.rs    RSS y descargas
src/media.rs    elementos de cola, lector de flujos HTTP y metadatos ICY
src/ipc.rs      daemon y cliente por socket Unix
src/app.rs      estado de la interfaz y acciones
src/ui.rs       las 12 vistas
src/widgets.rs  espectro y portadas generadas
tests/tui.rs    pruebas de interfaz con TestBackend
```

## Visualizador

Pulsa `b` para cambiar entre 10 estilos (se guarda en `visualizer` de los ajustes):
Segmentos, Mosaico, Led, Medidor, Matriz, Espejo, Braille, Onda, Pulso y Aurora.

Iconos: `▤`/`▥` carpetas (Biblioteca), `◉` canales de podcast, `▷` episodios, `📻` emisoras de radio.

El importador de playlists abre un navegador flotante: puedes recorrer todo el sistema de archivos (incluidos `/run/media`, `/mnt` y otros montajes), entrar en carpetas y seleccionar directamente un archivo compatible. Usa ↑/↓, Enter, Backspace y Esc.

## Temas de color

En Ajustes → Temas tienes una sección dedicada; pulsa `T` para recorrer: nocturne, daylight, matrix, cyberpunk, dracula, nord, gruvbox,
tokyo-night, catppuccin, synthwave, oceano y atardecer. También con `MUZORYN_THEME=matrix`
o `theme = "matrix"` en los ajustes.


## Radio Browser (6.7.0-Pro)

En Radio, pulsa **Tab** para abrir el submenú **Explorar Radio Browser**. Pulsa **/** para buscar emisoras por nombre directamente o con los filtros `country:`, `language:` y `genre:` (también `país:`, `idioma:` y `género:`). Selecciona un resultado con **↑/↓** y pulsa **Enter** para reproducirlo y guardarlo en **Mis emisoras**. Si ya estaba guardado, se reproduce sin duplicarlo. Pulsa **Tab** para volver a tus emisoras. La búsqueda requiere conexión a Internet; la lista local sigue disponible sin conexión.

Los filtros y sus valores se pueden escribir en español: `género:salsa país:cuba idioma:español` equivale
a `genre:salsa country:cuba language:Spanish`. MUZORYN traduce los idiomas (español, inglés, francés…) y
los géneros más comunes (clásica, noticias, deportes, reguetón…) al nombre que usa Radio Browser; los países
ya se reconocían en ambos idiomas. Lo que no reconoce se envía tal cual.
