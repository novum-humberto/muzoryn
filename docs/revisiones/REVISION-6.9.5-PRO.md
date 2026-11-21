# MUZORYN 6.9.5 Pro — paso 4 hacia 7.0: subventana de descargas

## Alcance
Solo el punto 4: «Crear la subventana de descargas».

### Hecho
- `Ctrl+D` o `F3` (acción `downloads`, reasignable en keys.toml) abre una ventana flotante con todas las descargas: canal completo, episodios activos y en cola, con estado, bytes y barra por entrada.
- Título con el recuento de activas y en cola; ↑/↓ desplazan; Esc, `Ctrl+D`, `?`, `H` o `w` la cierran. Es modal, como la ayuda.
- Se actualiza en vivo con el progreso del gestor. Ayuda, manual y README actualizados.
- Tests `downloads_window_lists_jobs_scrolls_and_closes` y `downloads_key_is_bound_by_default`.

### Aún no hecho
Barra global de progreso, diálogo paralela/cola, cancelación individual, recuperación de errores, conexión con podcasts (unificar `D`).

## Verificación
- NO compilado ni ejecutado (sin `cargo` en el entorno de preparación). Pendiente: `cargo build && cargo test`.
