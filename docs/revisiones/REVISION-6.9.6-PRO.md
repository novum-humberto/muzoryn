# MUZORYN 6.9.6 Pro — paso 5 hacia 7.0: barras de progreso individuales y general

## Alcance
Solo el punto 5: «Añadir barras de progreso individuales y general».

### Hecho
- Barra global TOTAL: media de las fracciones de todas las entradas (canal completo + episodios, activos y en cola). Una entrada en cola o sin tamaño conocido cuenta 0, así que el total nunca adelanta al progreso real.
- Se muestra fija arriba en la ventana `Ctrl+D` (con % y nº de descargas), en el panel DESCARGAS cuando hay más de una entrada y en el % de la barra inferior (antes era el del primer trabajo).
- Barras individuales: ya existían (panel y ventana); ahora comparten el mismo cálculo del canal completo.
- Código: `downloads::overall`, `DownloadManager::overall_fraction`; tests `overall_is_mean_with_queued_counting_zero` y ampliado el de la ventana.

### Aún no hecho
Diálogo paralela/cola, cancelación individual, recuperación de errores, conexión con podcasts (unificar `D`).

## Verificación
- NO compilado ni ejecutado (sin `cargo`). Pendiente: `cargo build && cargo test`.
