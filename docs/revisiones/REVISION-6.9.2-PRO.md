# MUZORYN 6.9.2 Pro — paso 1 hacia 7.0: gestor de descargas integrado en la interfaz

## Alcance
Solo el punto 1 de lo pendiente: «Integrar el gestor de descargas en la interfaz».

### Hecho (cambios en el código)
- Nuevo módulo `src/downloads.rs` con `DownloadManager` y `DownloadJob`: ids únicos, progreso, fracción/porcentaje y `is_full()` (constante `MAX_PARALLEL = 8`).
- `App` usa `app.dl` (gestor) en lugar de `Vec<ActiveDownload>` + `next_dl_id`.
- La interfaz (panel DESCARGAS y porcentaje de la barra inferior) lee del gestor.
- `tests/tui.rs` actualizado a la nueva API; 3 tests unitarios nuevos en `downloads.rs`.

### Sin cambios (a propósito)
- Comportamiento visible igual que 6.9.1. Aún NO hay cola real, subventana, selección múltiple, diálogo paralela/cola, cancelación individual ni recuperación de errores.

## Verificación
- NO compilado ni ejecutado: este entorno no tiene `cargo` ni red. Revisión estática únicamente (sin referencias antiguas a `downloads`/`ActiveDownload`/`next_dl_id`).
- Pendiente: `cargo build && cargo test` en tu máquina.
