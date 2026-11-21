# MUZORYN 7.0.0-beta.1 — entrega de revisión

## Cambios incluidos en esta entrega
- Se normalizan listas M3U/TOML que llegan con saltos de línea escapados literalmente (`\\n`), además de archivos con saltos de línea normales.
- Se corrige el tratamiento del BOM UTF-8 en los lectores M3U/TOML.
- Se incorpora PLS al importador/exportador de listas musicales.
- Se añade `radio_playlist_io`, módulo separado para importar/exportar emisoras en M3U/M3U8, PLS, XSPF y JSON. Filtra esquemas no HTTP(S), deduplica por URL y conserva el orden.
- La interfaz Radio queda conectada a la importación/exportación local: `i` abre el selector de archivos cuando estás en Radio; `Ctrl+E` exporta cuando estás en Radio.
- La importación de radio añade emisoras a las existentes y omite URLs duplicadas; no sustituye la lista guardada.
- La exportación crea automáticamente `exports/radio` dentro del directorio de datos de MUZORYN. Escribe M3U, M3U8, PLS, XSPF y JSON.
- En Mis emisoras se exportan las emisoras guardadas; en Radio Browser se exportan los resultados de la búsqueda actual.
- HTML sigue siendo un formato de playlist musical reconocido; su importación/exportación se mantiene en `playlist_io`.

## Alcance pendiente de esta beta
- La exportación de Radio Browser todavía exporta todos los resultados de la búsqueda actual; falta selección múltiple de resultados.
- La importación desde URL remota todavía no está conectada; en esta etapa se importan archivos locales.
- No se han alterado las funciones existentes de favoritos, recientes, reproducción ni búsqueda de Radio Browser.

## Verificación
El informe recibido corresponde a la versión base: `cargo check` y `cargo build --release` pasaron; `cargo test` dio 100/103. Los tres fallos indicados son los dos lectores M3U y TOML que recibían fixtures con `\\n` literal. Esta entrega contiene una corrección dirigida a ese caso y la integración de radio, pero no se ha podido ejecutar Cargo en el entorno de preparación. Por tanto, esta beta necesita `cargo fmt`, `cargo test` y `cargo build --release` en Linux antes de instalarse.

## Prueba local
```bash
cargo fmt --check
cargo test
cargo build --release
```

## Atajos y rutas de prueba de Radio
- En la vista Radio: `i` = importar archivo de emisoras.
- En la vista Radio: `Ctrl+E` = exportar.
- Radio Browser: exporta los resultados visibles de la búsqueda actual.
- Mis emisoras: exporta las emisoras guardadas.
- Destino: `~/.local/share/muzoryn/exports/radio/` (o el equivalente definido por `XDG_DATA_HOME` / `MUZORYN_DATA_DIR`).
- Formatos de radio: M3U, M3U8, PLS, XSPF y JSON.
- Para probar importación: seleccionar `country.m3u` desde el navegador de archivos de Radio.
