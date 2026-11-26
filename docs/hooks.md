# 🪝 Hooks

Un **hook** ejecuta un comando de shell cuando ocurre un evento del reproductor.

| Evento | Cuándo |
|---|---|
| `track_changed` | Cambia la canción |
| `playback_stopped` | Se detiene la reproducción |

Variables de entorno disponibles para tu comando: `MUZORYN_TITLE`, `MUZORYN_ARTIST`, `MUZORYN_ALBUM`. **Nunca** se insertan dentro del texto del comando: viajan como variables de entorno. Cita siempre las variables en tu comando (`"$MUZORYN_TITLE"`) para que un título con espacios o símbolos no altere la orden.

## Gestionarlos desde la terminal
```bash
muzoryn hooks                                      # lista los hooks y si son de confianza
muzoryn hooks add track_changed 'notify-send "$MUZORYN_TITLE" "$MUZORYN_ARTIST"'
muzoryn hooks remove 1                             # elimina el hook número 1
muzoryn hooks restore [--yes]                      # recupera los desactivados por permisos inseguros
```

O a mano en `settings.toml`:
```toml
[[hooks]]
event = "track_changed"
command = "notify-send \"$MUZORYN_TITLE\" \"$MUZORYN_ARTIST\""
```

`muzoryn hooks` se niega a editar si el archivo no es de confianza. En **Ajustes → Avanzado** se ve cuántos hooks hay activos y aparece un aviso si hay hooks en cuarentena.

## Seguridad
- **Confianza:** solo se activan si `settings.toml` es **tuyo**, **no es escribible** por grupo ni otros y su carpeta tampoco (salvo carpetas con *sticky bit*). Los enlaces simbólicos se resuelven antes de comprobar.
- **Cuarentena:** si no hay confianza, los hooks se desactivan y se copian a `settings.toml.hooks-en-cuarentena` (permisos `0600`). No se pierden ni se «blanquean» al guardar los ajustes. Para activarlos: arregla los permisos y ejecuta `muzoryn hooks restore --yes`.
- **Validación:** solo los eventos de la tabla; comando no vacío, de hasta 1024 caracteres y sin byte nulo; máximo 16 hooks.
- **Ejecución:** `sh -c` en su propio grupo de procesos, **límite de 30 s** (se mata todo el grupo) y máximo **8 simultáneos**.

Si tu `settings.toml` está en una carpeta con permisos de grupo/otros, arréglalo así:
```bash
chmod 700 ~/.config/muzoryn
chmod 600 ~/.config/muzoryn/settings.toml
```
