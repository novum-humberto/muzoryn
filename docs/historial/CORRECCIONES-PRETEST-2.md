# MUZORYN — correcciones para nueva ronda de pruebas

Esta carpeta parte del paquete `MUZORYN-7.0.0-seguridad-pretest` recibido para revisión.
No modifica ni sustituye la copia original 7.0.0.

## Correcciones aplicadas según `MUZORYN-RESULTADOS-TEST.txt`

1. `src/config.rs`: se incorpora `PermissionsExt` al ámbito para habilitar `Permissions::from_mode(0o600)` en Unix.
2. `src/media.rs`: se elimina el estado `failed` redundante. Las dos salidas que rompen el bucle son rutas de reintento, por lo que el contador se incrementa al salir del bucle.
3. `src/radio_playlist_io.rs`: el título XSPF pasa a ser local a cada pista; se elimina la asignación inicial sobrescrita.
4. `src/ui.rs`: se sustituyen las dos comparaciones `width >= longitud + 1` por la condición equivalente `width > longitud`, señalada por Clippy.

## Prueba en Arch

Desde esta carpeta, ejecuta primero `cargo fmt` (formatea el árbol de trabajo) y después: `cargo fmt --check`, `cargo test --locked` y `cargo clippy --locked --all-targets -- -D warnings`.

No se ha podido ejecutar Rust/Cargo en el entorno de preparación porque no tiene `rustc` ni `cargo` instalados. Por tanto, estas correcciones son cambios de código revisados, no una compilación validada. El resultado de tu máquina es el que determinará si pasan.
