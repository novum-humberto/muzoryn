# 📚 Biblioteca y listas

## La biblioteca
MUZORYN indexa las carpetas de `music_dirs` (en `settings.toml`) en una base SQLite (`~/.local/share/muzoryn/library.db`). El escaneo es incremental (por fecha de modificación) y lee etiquetas: título, artista, álbum, género, número de pista, duración y ReplayGain.

Formatos: mp3, flac, ogg/opus, wav, m4a/aac, aiff.

Pulsa `u` para actualizar tras añadir música.

## Gestionar las carpetas de música (`Alt+M`)
En el **primer inicio** se abre este navegador para que elijas tu música; después se abre en cualquier momento con `Alt+M` desde la interfaz.

| Tecla | Acción |
|---|---|
| `↑`/`↓` | Mover |
| `Enter` | Entrar en la carpeta |
| `Backspace` | Subir al directorio padre |
| `s` | **Añadir** la carpeta actual a la biblioteca |
| `x` | **Quitar** la raíz seleccionada o actual de la biblioteca |
| `Esc` | Cerrar |

- Puedes recorrer todo el sistema de archivos, incluidos `/media`, `/mnt` y `/run/media`, para localizar memorias USB y discos externos.
- **Quitar una carpeta no borra nada del disco:** MUZORYN solo elimina del índice esas pistas, sus favoritos y sus referencias en listas. Una carpeta con nombre parecido no se ve afectada.
- Las rutas se guardan en `music_dirs`; también puedes editarlas a mano.

## Navegar y buscar
- **Biblioteca (2):** árbol de carpetas a cualquier profundidad con contador de pistas.
- **Búsqueda (3):** canciones, álbumes y artistas a la vez.
- **Álbumes (5):** cuadrícula con portadas; pestañas Álbumes / Artistas / Géneros.

## Listas (6)
Favoritas y listas propias. `Ctrl+N` crea, `F2` renombra, `x` quita.

### Selección múltiple
1. En **Listas (6)** elige la lista destino.
2. Marca con `m` (otra vez para desmarcar): en Biblioteca una pista o carpeta entera, en Álbumes un álbum/artista/género, en Búsqueda pistas, álbumes o artistas, y en Cola la pista seleccionada.
3. Las marcadas muestran `+` y el encabezado indica cuántas llevas; se conservan al cambiar de vista.
4. `l` las añade todas a la lista. `M` quita las marcas. Sin marcas, `l` añade solo lo seleccionado.

### Importar y exportar listas
En Listas, `i` abre un navegador de archivos (el formato se detecta por la extensión) y `Ctrl+E` exporta: escribe el formato o déjalo vacío para generar todos.

| Formato | Notas |
|---|---|
| **M3U / M3U8** | Decodifica rutas URL-encoded y `file://`; ignora streams HTTP; conserva entradas aunque el archivo no esté disponible; mantiene los metadatos `#EXTINF` en las entradas nuevas |
| **PLS** | Guarda el nombre de la lista en `PlaylistName` y lo recupera al importar |
| **TOML** | Importación tolerante |
| **XSPF** | XML estándar (VLC, Clementine…) |
| **HTML** | Se abre en el navegador y también se reimporta |

Destino de la exportación: `~/.local/share/muzoryn/exports/`. Los archivos con saltos de línea escapados literalmente (`\n`) y con BOM UTF-8 se leen correctamente.

Las **emisoras** tienen su propio importador/exportador: ver [Radio](radio.md).

## Letras
Coloca `cancion.lrc` junto a `cancion.flac`. En la vista Letras (9), `[` y `]` ajustan el desfase.
