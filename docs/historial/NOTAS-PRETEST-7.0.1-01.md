# MUZORYN 7.0.1 — paquete de pretest 01

Este paquete es una copia de trabajo independiente derivada de MUZORYN 7.0.0. No sustituye ni modifica la versión estable original.

## Correcciones incluidas

- PLS: el exportador ahora guarda el nombre de la playlist en `PlaylistName` y el importador lo recupera. Es un campo adicional tolerable para lectores PLS que no lo utilicen; las entradas `FileN` mantienen su orden.
- Clippy: correcciones puntuales en `app.rs`, `core.rs`, `radio_playlist_io.rs`, `ui.rs`, `widgets.rs`, `config.rs`, `library.rs`, `media.rs` y `podcast_directory.rs` para los 18 diagnósticos del informe de pruebas 2.
- Los cambios de Clippy son refactorizaciones equivalentes; no se han introducido funciones nuevas.

## Deliberadamente no incluido

- No se han modificado el daemon ni el sistema de snapshots.
- No se han aplicado todavía las cuatro tareas de seguridad (socket IPC, permisos, hooks y descargas). Se harán en una fase separada después de validar esta base.
- No se ha cambiado la versión declarada en `Cargo.toml`; el incremento a 7.0.1 se hará al cerrar y validar la versión de mantenimiento.

## Verificación pendiente

Este entorno de preparación no dispone de `cargo`, `rustc` ni `rustfmt`. Por tanto, este paquete no se presenta como compilado ni probado. En Arch Linux, desde esta carpeta, ejecutar:

```bash
./verificar-pretest.sh
```

El script aplica el formato con `cargo fmt`, comprueba el formato, ejecuta los tests bloqueados y Clippy con advertencias tratadas como errores. Guarda la salida completa en `MUZORYN-RESULTADOS-PRETEST-01.txt`.

Si alguna fase falla, conservar y compartir ese TXT. No instalar este pretest como reemplazo de MUZORYN 7.0.0.
