# ⚙️ Configuración

## Dónde se guarda todo
| Qué | Dónde |
|---|---|
| Ajustes | `~/.config/muzoryn/settings.toml` |
| Atajos | `~/.config/muzoryn/keys.toml` |
| Biblioteca | `~/.local/share/muzoryn/library.db` |
| Podcasts descargados | `~/.local/share/muzoryn/podcasts/<canal>/` |
| Exportaciones | `~/.local/share/muzoryn/exports/` (emisoras en `exports/radio/`) |
| Hooks en cuarentena | `~/.config/muzoryn/settings.toml.hooks-en-cuarentena` |
| Socket del daemon | `$XDG_RUNTIME_DIR/muzoryn.sock` |

La primera vez se crea `settings.toml` y se abre el navegador de carpetas de música. Si el archivo tiene sintaxis inválida, MUZORYN lo informa antes de abrir la interfaz; renómbralo (`mv settings.toml settings.toml.bak`) y se creará uno nuevo. Para conservar los ajustes de la época *Sonora*, copia `~/.config/sonora/settings.toml` a `~/.config/muzoryn/`.

## Ajustes desde la interfaz
`,` abre **Ajustes**: General, Audio, Biblioteca, Daemon/IPC, Avanzado, Acerca de y Temas. En *Avanzado* se ven los hooks, el número de emisoras y podcasts, las rutas de descargas y exportaciones y el máximo de descargas paralelas (`←`/`→` para ajustarlo).

## Ejemplo de `settings.toml`
```toml
theme = "nocturne"
music_dirs = ["/home/yo/Music", "/mnt/disco/Musica"]
podcast_country = "mx"
radio_click_count = true
parallel_downloads = 8

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

## Opciones principales
| Clave | Qué hace |
|---|---|
| `theme` | Tema de color (nombre o ruta a un `.toml`) |
| `visualizer` | Estilo del visualizador (se guarda al pulsar `b`) |
| `volume` | Volumen |
| `eq_enabled` · `eq` | Ecualizador activo y sus 10 bandas |
| `replay_gain` | ReplayGain |
| `crossfade_secs` | Crossfade en segundos |
| `shuffle` · `repeat` | Aleatorio y repetición |
| `music_dirs` | Carpetas que indexa la biblioteca |
| `podcast_country` | Tienda del directorio de podcasts (2 letras; por defecto `mx`) |
| `radio_click_count` | `false` desactiva el aviso de clics a Radio Browser |
| `parallel_downloads` | Descargas simultáneas de episodios (1–8) |
| `recent_stations` | Historial de emisoras (lo gestiona la app) |
| `last_view` · `last_focus` | Última vista y panel (para volver al mismo sitio) |
| `[[stations]]` | Emisoras: `name`, `url`, `bitrate`, y rellenados solos `fav`, `uuid`, `tags` |
| `[[podcasts]]` | Podcasts: `name`, `url` |
| `[[hooks]]` | Hooks: `event`, `command` ([ver Hooks](hooks.md)) |

Después de cambiar `music_dirs` a mano, abre MUZORYN y pulsa `u` para re-escanear.

## Variables de entorno
| Variable | Para qué |
|---|---|
| `MUZORYN_THEME` | Tema (nombre o ruta a `.toml`) |
| `MUZORYN_KEYS` | Archivo de atajos |
| `MUZORYN_SOCKET` | Ruta del socket del daemon |
| `MUZORYN_CONFIG_DIR` / `MUZORYN_DATA_DIR` | Cambiar las carpetas de configuración y de datos |
| `MUZORYN_GRAPHICS` | `kitty` · `iterm` · `sixel` · `blocks` |
| `MUZORYN_NO_OSC=1` | No cambiar el color de fondo de la terminal |

## Permisos
MUZORYN crea su carpeta de configuración con permisos `0700`, `settings.toml` con `0600` y el socket del daemon con `0600`.
