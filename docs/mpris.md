# ⏯️ MPRIS 2: teclas multimedia, `playerctl` y widgets del escritorio

MUZORYN incluye soporte de **MPRIS 2**, el estándar de Linux para controlar reproductores desde fuera. Gracias a él, las **teclas multimedia** de tu teclado, `playerctl`, los widgets de GNOME/KDE y módulos de barra como Waybar controlan MUZORYN **sin que configures nada**.

> 🧪 **Estado:** funcionalidad nueva; compila y pasa las pruebas automáticas, pendiente de validación manual en Linux. Está activada por defecto; si algo falla, mira [Solución de problemas](solucion-de-problemas.md#mpris-no-aparece-en-playerctl) o compila sin ella (más abajo).

## Cómo funciona
- MPRIS corre **dentro del daemon** (`muzoryn --daemon`, o el que arranca solo al abrir `muzoryn`), en hilos propios.
- Se registra en el **bus de sesión de D-Bus** con el nombre `org.mpris.MediaPlayer2.muzoryn`.
- No necesita librerías del sistema adicionales: usa `zbus`, una implementación de D-Bus escrita en Rust.
- Si no hay bus de sesión (por ejemplo, una sesión SSH sin entorno gráfico), el daemon **sigue funcionando sin MPRIS** y avisa por `stderr`.

## Probarlo
```bash
muzoryn --daemon &                  # o abre muzoryn normalmente
playerctl -p muzoryn status         # Playing / Paused / Stopped
playerctl -p muzoryn metadata       # título, artista, álbum, duración
playerctl -p muzoryn play-pause
playerctl -p muzoryn next
playerctl -p muzoryn previous
playerctl -p muzoryn position 30    # salta al segundo 30
playerctl -p muzoryn volume 0.5     # volumen al 50 %
playerctl -p muzoryn shuffle toggle
playerctl -p muzoryn loop Playlist  # None · Track · Playlist
```

## Qué soporta
| Interfaz | Soportado |
|---|---|
| Órdenes | `Play`, `Pause`, `PlayPause`, `Stop`, `Next`, `Previous`, `Seek`, `SetPosition` |
| Propiedades | `PlaybackStatus`, `LoopStatus`, `Shuffle`, `Volume`, `Metadata`, `Position`, `CanGoNext`, `CanGoPrevious`, `CanPlay`, `CanPause`, `CanSeek`, `CanControl` |
| Señales | `PropertiesChanged` al cambiar estado, pista, volumen, repetición o aleatorio |
| Radio | `CanSeek` es falso en emisoras en directo (los *streams* no admiten *seek*); el título mostrado es la canción en vivo (ICY) |

**Metadatos publicados:** `mpris:trackid`, `mpris:length`, `xesam:title`, `xesam:artist`, `xesam:album` y, para pistas por URL (radio y podcasts), `xesam:url`.

**No soporta (por ahora):** `TrackList`, `OpenUri`, cambio de `Rate` (siempre 1.0) ni la señal `Seeked`. `Raise` y `Quit` no hacen nada (`CanRaise` y `CanQuit` son falsos); para cerrar el daemon usa `muzoryn ctl quit`.

## Compilar sin MPRIS
MPRIS es la *feature* `mpris` de Cargo y está en las predeterminadas. Para quitarla:

```bash
cargo build --release --no-default-features
```

También se compila fuera de Unix sin ella. Con `--no-default-features` vuelves a enlazar las teclas con `muzoryn ctl` ([Daemon y control externo](daemon-y-control.md#teclas-multimedia)).

## Privacidad
Cualquier programa de tu sesión de usuario puede leer los metadatos y controlar la reproducción por D-Bus; es el comportamiento normal de MPRIS. Para pistas por URL, `xesam:url` contiene la dirección completa: si la de un podcast lleva un token personal, otros programas de tu sesión podrían verla. Más en [Seguridad](seguridad.md).
