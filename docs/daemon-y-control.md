# 🧩 Daemon y control externo

Al abrir `muzoryn`, el **daemon** de audio arranca solo en segundo plano (comportamiento tipo MOC). La interfaz y el daemon se comunican por un socket Unix.

| Tecla | Efecto |
|---|---|
| `w` | Cierra la interfaz; **la música sigue**. Al volver a abrir `muzoryn`, aparece el mismo panel (Biblioteca, Radio, etc.) |
| `W` o `Ctrl+C` | Cierra la interfaz y **detiene** el daemon |

## Línea de comandos
```bash
muzoryn                           # abre la interfaz (inicia el daemon si hace falta)
muzoryn ~/Music/Album             # añade archivos o carpetas a la cola y reproduce
muzoryn --daemon [rutas...]       # daemon sin interfaz
muzoryn ctl <orden>               # controla un daemon activo
muzoryn hooks [list|add|remove|restore]
muzoryn --help | -h               # ayuda breve
muzoryn --man                     # manual completo
muzoryn --version                 # versión instalada
```

Órdenes de `ctl`: `toggle` · `next` · `prev` · `stop` · `vol+` · `vol-` · `shuffle` · `repeat` · `forward` · `back` · `quit` · `status`.

`muzoryn ctl quit` hace un **cierre ordenado**: detiene el audio, espera al hilo, borra el socket y termina con código 0.

## Actualizar el programa
Cierra el daemon antiguo antes de usar la versión nueva (`muzoryn ctl quit`). Un cliente antiguo con un daemon nuevo (o al revés) puede mostrar la cola vacía, porque el campo `queue_same` es opcional.

## Rendimiento (7.0.1)
El daemon ya no reenvía un estado idéntico y envía la cola solo cuando cambia. Cadencia: **100 ms** mientras suena y **250 ms** en pausa o parado (`SNAPSHOT_ACTIVE` / `SNAPSHOT_IDLE` en `src/ipc.rs`). Con la interfaz abierta y el daemon parado, una orden puede tardar hasta 250 ms en reflejarse.

Con colas de miles de pistas conviene usar la interfaz integrada.

## Teclas multimedia
MUZORYN ofrece MPRIS 2 si se compila con `--features mpris` (`playerctl`, teclas multimedia, widgets del escritorio). Sin esa opción, enlaza las teclas de tu escritorio con `muzoryn ctl`.

**Hyprland / Omarchy**
```conf
bindl = , XF86AudioPlay, exec, muzoryn ctl toggle
bindl = , XF86AudioNext, exec, muzoryn ctl next
bindl = , XF86AudioPrev, exec, muzoryn ctl prev
bindl = , XF86AudioStop, exec, muzoryn ctl stop
```

**sway / i3**
```conf
bindsym XF86AudioPlay exec muzoryn ctl toggle
bindsym XF86AudioNext exec muzoryn ctl next
bindsym XF86AudioPrev exec muzoryn ctl prev
bindsym XF86AudioStop exec muzoryn ctl stop
```

## Nombres de proceso
En Linux el proceso principal se llama `muzoryn` y el daemon `muzoryn-daemon` (visibles con `pgrep -a muzoryn`).
