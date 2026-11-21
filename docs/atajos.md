# ⌨️ Atajos de teclado

Dentro de la app: `?` o `F1` muestran la ayuda de atajos y `H` abre el manual integrado. Fuera: `muzoryn --man`.

## Navegación
| Atajo | Acción |
|---|---|
| `1`–`9`, `0` | Vistas: Inicio, Biblioteca, Búsqueda, Cola, Álbumes, Listas, Radio, Podcasts, Letras, Audio y EQ |
| `v` · `,` | Vista completa · Ajustes |
| `↑`/`↓` o `k`/`j` | Mover la selección |
| `Tab` | Cambiar de panel o pestaña |
| `Enter` | Abrir / reproducir |
| `Esc` | Volver o cerrar ventanas flotantes |
| `/` | Buscar |

## Reproducción
| Atajo | Acción |
|---|---|
| `Espacio` | Pausa / reanuda |
| `s` | Parar (en radio y podcasts también cancela una conexión en curso) |
| `n` / `p` | Siguiente / anterior |
| `←` / `→` | Retroceder / avanzar 5 s |
| `=` o `+` / `-` | Subir / bajar volumen |
| `z` | Aleatorio |
| `o` | Repetir (no / todo / una) |
| `c` | Crossfade (0 / 3 / 6 / 9 s) |
| `g` | ReplayGain |
| `e` / `t` | Preset del ecualizador / ecualizador on-off |
| `b` | Cambiar de visualizador (10 estilos) |
| `T` | Siguiente tema · `r` recargar el tema |
| `[` / `]` | Ajustar el desfase de la letra |

## Biblioteca, cola y listas
| Atajo | Acción |
|---|---|
| `a` | Añadir a la cola |
| `f` | Favorito |
| `x` o `Supr` | Quitar |
| `Alt+↑` / `Alt+↓` | Mover en la cola |
| `Ctrl+X` | Vaciar la cola |
| `m` / `M` | Marcar o desmarcar / quitar todas las marcas |
| `l` | Añadir lo marcado (o lo seleccionado) a la lista elegida |
| `Ctrl+N` | Nueva lista |
| `F2` | Renombrar lista |
| `i` | Importar (listas o, en Radio, emisoras) |
| `Ctrl+E` | Exportar (listas o, en Radio, emisoras) |
| `u` | Actualizar biblioteca / feed |
| `Alt+M` | Gestionar carpetas de música |

## Radio y podcasts
| Atajo | Acción |
|---|---|
| `a` o `Ctrl+A` | Añadir emisora o feed por URL |
| `f` | Favorita ★ (Radio) / suscribirse (directorio de podcasts) |
| `Ctrl+B` | Abrir el directorio de podcasts |
| `d` | Descargar episodio (o todos los marcados) |
| `D` | Descargar el canal completo |
| `Ctrl+D` o `F3` | Gestor de descargas |

## Salir
| Atajo | Acción |
|---|---|
| `w` | Cierra la interfaz; el daemon sigue reproduciendo |
| `W` o `Ctrl+C` | Cierra la interfaz y detiene el daemon |

## Cambiar los atajos

```bash
mkdir -p ~/.config/muzoryn
cp config/keys.toml ~/.config/muzoryn/keys.toml
```

```toml
[keys]
next = ["n", "ctrl+right"]      # un atajo propio REEMPLAZA todos los de esa acción
quit_all = ["W", "ctrl+c"]
```

Otro archivo: `MUZORYN_KEYS=/ruta/mis-atajos.toml muzoryn`. Las acciones disponibles están listadas en [`config/keys.toml`](../config/keys.toml).
