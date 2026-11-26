# Historial de cambios

Formato libre, de lo más reciente a lo más antiguo. Las notas internas de cada entrega están en [`docs/historial/`](docs/historial/) y [`docs/revisiones/`](docs/revisiones/).

## Sin publicar

- **Listas · importar:** el explorador vuelve a mostrar los archivos `.pls`; los archivos con codificación no UTF-8 se leen sin abortar; si el archivo no contiene rutas locales se avisa en lugar de crear una lista vacía.
- **Calidad:** corregidos los dos errores de Clippy en `src/mpris.rs` (import sin uso y `let` sobre valor unitario).
- **Listas · exportar:** `Ctrl+E` (o *Exportar lista* en Acciones) usa el mismo flujo que Radio: primero pide el nombre del archivo y después la extensión (`m3u`, `m3u8`, `pls`, `toml`, `xspf`, `html`; vacío = todos). Avisa si la lista está vacía y no sobrescribe archivos previos (añade `-2`, `-3`…).
- **Listas · añadir/quitar contenido:** nuevas acciones *Añadir carpeta/canción* y *Quitar carpeta/canción*. En el explorador, `Enter` entra en carpetas o aplica la acción a una canción; `s` (añadir) / `x` (quitar) la aplican a la carpeta o canción resaltada. Funciona también con «Favoritas».

## 7.0.1 — mantenimiento y seguridad

**Seguridad y robustez**
- **Hooks:** solo se activan si `settings.toml` es del usuario y ni el archivo ni su carpeta son escribibles por grupo/otros; si no, pasan a cuarentena (`settings.toml.hooks-en-cuarentena`) sin perderse. Validación de eventos y comandos, límite de 30 s (se mata todo el grupo de procesos), máximo 8 simultáneos y 16 en total. Nuevo comando `muzoryn hooks list|add|remove|restore`.
- **Redirecciones HTTP:** se siguen a mano (máx. 5 saltos), se bloquea HTTPS→HTTP y cualquier esquema que no sea `http(s)`. Afecta a todos los clientes (radio, podcasts, directorios, descargas, carátulas).
- **Instalador:** ya no ejecuta `curl | sh`. Si falta Rust pide confirmación (o `--yes`), descarga `rustup-init` y su `.sha256` por HTTPS y verifica la suma antes de ejecutarlo.
- **Daemon:** `ctl quit` hace un cierre ordenado (detiene el audio, espera al hilo, borra el socket y sale con código 0).
- **Snapshots IPC:** no se reenvía un estado idéntico; la cola solo viaja cuando cambia; cadencia de 100 ms sonando y 250 ms en pausa o parado.
- **Descargas:** límites de conexión (15 s), silencio del servidor (30 s), duración total (2 h) y velocidad mínima (16 KiB por ventana de 60 s); se borra el `.part` al cortar.
- **Nombres repetidos:** dos episodios con el mismo título ya no se pisan (`Título [hash].ext`) y cada canal guarda un índice `.muzoryn-index.tsv`.
- **Descargas:** `d` no repite episodios ya descargados o en cola; hasta 3 reintentos (esperas de 1 s y 2 s) ante fallos de red, HTTP 429 y 5xx; mensaje de error con el código HTTP.

**Interfaz y formatos**
- Los atajos `u · actualizar feed` y `u · recargar populares` ya no quedan ocultos en paneles estrechos.
- PLS: el exportador guarda el nombre de la lista (`PlaylistName`) y el importador lo recupera.
- Correcciones de Clippy sin cambios funcionales.
- 178 pruebas automáticas (145 unitarias + 33 de interfaz).

## 7.0.0

- **Importar y exportar emisoras** en Radio (`i` y `Ctrl+E`): M3U/M3U8, PLS, XSPF y JSON; filtra esquemas no HTTP(S), omite URLs duplicadas y conserva el orden.
- **PLS** en las listas musicales; el importador normaliza saltos de línea escapados y el BOM UTF-8.
- **Podcasts en streaming** con búfer progresivo (canal acotado de 512 bloques de 8 KiB, unos 4 MiB) y reanudación con HTTP Range cuando el servidor lo permite.
- **Barra de progreso** de podcasts basada en `itunes:duration`.
- **Gestor de carpetas de música:** en el selector, `s` añade la carpeta actual y `x` quita la raíz actual del índice (sin borrar archivos del disco).
- Descargas de podcasts organizadas por canal.
- En Linux el proceso se llama `muzoryn` y el daemon `muzoryn-daemon`.

## 6.9.x — camino hacia 7.0

| Versión | Cambio |
|---|---|
| 6.9.14 | Importación de listas externas: M3U decodifica rutas URL-encoded y `file://`, ignora streams HTTP y conserva entradas aunque el archivo no esté |
| 6.9.13 | Indicador compacto de descargas en la barra lateral; `Alt+M` visible en Biblioteca; `? Atajos` en el menú lateral |
| 6.9.12 | Bibliotecas configurables desde la interfaz: selección de carpeta en el primer inicio y `Alt+M` (USB y discos externos) |
| 6.9.11 | Máximo de descargas paralelas (1–8) configurable con `←`/`→`; `Tab` en Podcasts alterna canales/episodios |
| 6.9.9 | `D` añade el canal completo al mismo gestor que `d` |
| 6.9.8 | Con 2 o más episodios marcados, `d` pregunta: en paralelo o en cola |
| 6.9.6 | Barra global TOTAL en el gestor de descargas |
| 6.9.5 | `Ctrl+D` / `F3` abre la ventana de descargas |
| 6.9.4 | `m` marca episodios, `M` quita marcas, `d` descarga los marcados |
| 6.9.3 | Máximo de 8 descargas activas; el resto espera en cola |
| 6.9.2 | Gestor de descargas propio (`src/downloads.rs`) |

## 6.9.1 y anteriores

- **6.9.1:** `D` descarga el canal completo de un podcast.
- **6.9.0:** explorador de podcasts en español (búsqueda, populares y 19 categorías).
- **6.8.x:** favoritas ★, historial, canción en vivo (ICY), reconexión automática y estados de conexión en Radio; flujos abiertos en segundo plano con búfer de ~1 MiB; filtros de Radio Browser en español.
- **6.7.0:** Radio Browser.
- **0.6.x:** carátulas reales, Sixel, importación/exportación de listas, navegación de Biblioteca por carpetas. El proyecto pasó de llamarse *Sonora* a **MUZORYN**.
