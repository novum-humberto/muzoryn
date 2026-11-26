# MUZORYN 6.9.14-Pro — corrección de importación de playlists

## Diagnóstico confirmado en el código 6.9.13-Pro

- `parse_m3u` trataba las rutas M3U como texto literal y no decodificaba escapes porcentuales como `%20`, `%C3%B3` o `%28`.
- La importación añadía a la playlist únicamente pistas ya existentes en la base de datos de la biblioteca. Las rutas no reconocidas se descartaban.
- La limpieza de biblioteca podía eliminar referencias a archivos ausentes, incluso si estaban guardados en una playlist.
- Los M3U con streams HTTP podían interpretarse como rutas locales inexistentes.

## Cambios

- Decodificación UTF-8 de rutas M3U codificadas por porcentaje; soporte de rutas `file://`.
- Omisión de entradas HTTP/HTTPS, que corresponden a streams remotos y no a archivos de música locales.
- Conservación de las pistas no indexadas como registros de referencia con título derivado del archivo.
- Conservación de metadatos `#EXTINF` (título, artista y duración) para entradas nuevas.
- La limpieza automática ya no elimina pistas ausentes que estén referenciadas por una playlist.
- Las coincidencias por ruta y por nombre único de archivo siguen funcionando.

## Sobre los archivos adjuntos

- `aaaaa.m3u`: contiene rutas relativas con escapes porcentuales y metadatos `#EXTINF`; cubre el caso de decodificación.
- `jazz_completo-si.m3u` y `joaquicito-este-si.m3u`: contienen rutas absolutas sin cabecera `#EXTM3U`; el lector sigue admitiendo ese formato sencillo.
- `Joaquin Sabina.toml` y `jazz.toml`: contienen únicamente bloques `[[dir]]`, por lo que son configuraciones de directorios, no listas de pistas.
- `Joaqui Sabina.toml`, `Jazz.toml (1)` y `jazz_completo-si.toml`: contienen bloques `[[track]]`, que el importador TOML reconoce.

## Verificación

- Se añadieron pruebas unitarias para rutas M3U codificadas, metadatos EXTINF y retención de referencias ausentes en playlists.
- ZIP e integridad CRC se verificarán al empaquetar.
- `cargo fmt`, `cargo test` y `cargo build --release` no se han podido ejecutar en este entorno porque no dispone de Rust/Cargo.
