# Corrección preliminar de MUZORYN 6.9.14-Pro

Se corrigieron dos literales de carácter inválidos en `src/playlist_io.rs`:
- `m3u_metadata`: el BOM Unicode se expresa como ` '\u{feff}' ` en Rust.
- `toml_metadata`: la misma corrección para el análisis TOML.

La secuencia anterior contenía dos barras invertidas en el literal de carácter y Rust la interpretaba como varios caracteres, provocando:
`character literal may only contain one codepoint`.

Se añadió una prueba unitaria para verificar la lectura del BOM en metadatos M3U y TOML.

IMPORTANTE: esta entrega es una corrección del código fuente. No se pudo ejecutar `cargo check`, `cargo test` ni `cargo build` en el entorno de preparación porque no están instalados `cargo` ni `rustc`. Por ello, no se certifica todavía la compilación ni la instalación en el equipo de destino.
