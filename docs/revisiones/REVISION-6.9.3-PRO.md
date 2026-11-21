# MUZORYN 6.9.3 Pro — paso 2 hacia 7.0: límite real de 8 descargas simultáneas

## Alcance
Solo el punto 2: «Implementar el límite real de ocho trabajos simultáneos».

### Hecho
- `DownloadManager` distingue trabajos `Queued` y `Active`; `next_to_start()` nunca deja más de `MAX_PARALLEL = 8` activos.
- `d` ya no rechaza la novena descarga: la pone en cola y arranca sola cuando termina otra (`App::pump_downloads`).
- El panel DESCARGAS marca los trabajos en espera como «en cola».
- Tests unitarios: máximo 8 activos, orden de la cola, arranque al liberarse un hueco.

### Alcance del límite
- Cuenta solo las descargas sueltas de episodios (`d`). La descarga de canal completo (`D`) conserva su propio grupo de hilos y aún no entra en este límite; se unificará al conectar con el sistema de podcasts.

### Aún no hecho
Selección múltiple, subventana, barra global, diálogo paralela/cola, cancelación individual, recuperación de errores.

## Verificación
- NO compilado ni ejecutado (sin `cargo` en el entorno de preparación). Pendiente: `cargo build && cargo test`.
