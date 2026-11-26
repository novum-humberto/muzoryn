# 🧪 Desarrollo y pruebas

## Compilar
Requisitos: Rust ≥ 1.82, compilador de C, y en Linux `libasound2-dev` y `pkg-config`.

```bash
cargo build --release
cargo test
./target/release/muzoryn
```

## Pruebas

Ejecuta un único comando desde la raíz del proyecto:

```bash
./test.sh
```

Comprueba el formato sin modificar archivos, ejecuta todos los tests con MPRIS activado por defecto y ejecuta Clippy con los avisos tratados como errores. La primera compilación puede actualizar `Cargo.lock` para incorporar la dependencia de MPRIS; revisa y conserva ese cambio en Git.

**No cubierto por pruebas automáticas:** reproducción real en hardware, continuidad audible gapless, integración efectiva con el bus D-Bus del escritorio e instalación en cada distribución. Esas comprobaciones requieren una prueba manual en Linux.

## Estructura del código
```text
src/core.rs               cola, estado, comandos, aleatorio/repetición, crossfade, servicio
src/audio.rs              Rodio + cadena DSP (EQ, ganancia, espectro) + motor nulo
src/dsp.rs                biquad, presets y espectro
src/library.rs            SQLite, escaneo, favoritos, listas
src/playlist_io.rs        importar/exportar listas musicales (M3U, M3U8, PLS, TOML, XSPF, HTML)
src/radio_playlist_io.rs  importar/exportar emisoras (M3U/M3U8, PLS, XSPF, JSON)
src/lyrics.rs             parser LRC
src/feeds.rs              RSS y descarga de episodios (límites, reintentos, nombres)
src/downloads.rs          gestor de descargas (cola, límite, progreso)
src/media.rs              elementos de cola, lector HTTP con redirecciones controladas, ICY
src/radio_browser.rs      cliente de Radio Browser
src/podcast_directory.rs  directorio de podcasts
src/hooks.rs              confianza, cuarentena, validación y ejecución de hooks
src/ipc.rs                daemon y cliente por socket Unix
src/app.rs · ui.rs · widgets.rs · gfx.rs · cover.rs · theme.rs · keys.rs   interfaz
src/config.rs             ajustes y rutas
src/main.rs               arranque y línea de comandos
tests/tui.rs              pruebas de interfaz con TestBackend
```

## Contribuir
1. Haz un *fork* y crea una rama: `git checkout -b mi-mejora`.
2. Comprueba que todo pasa: `./test.sh`.
3. Abre un *Pull Request* explicando el cambio.

## Historial interno
Las notas de cada entrega están en [`historial/`](historial/) (betas, candidato 7.0.0, pretests 01–06 e informe 7.0.1) y [`revisiones/`](revisiones/) (6.9.x-Pro).
