# MUZORYN 6.9.10 Pro — Paso 9 de 10

## Sección: pruebas del funcionamiento completo del gestor de descargas

### Pruebas añadidas en `src/downloads.rs`
- `mixed_individual_marked_and_channel_jobs_share_one_eight_job_limit`: simula trabajos procedentes de descarga individual, selección múltiple y canal completo en el mismo gestor; verifica 8 activas como máximo y reposición FIFO.
- `queue_mode_applies_to_channel_jobs_too_and_never_starts_two`: comprueba que los episodios de un lote de canal también respetan el modo de una descarga activa.
- `cancelling_queued_or_active_job_does_not_break_the_queue`: comprueba la retirada de trabajos en espera y activos, y que el hueco se repone correctamente.
- `failed_job_is_removed_and_next_queued_job_can_start`: comprueba que un trabajo fallido finalizado no bloquea la cola.
- `parallel_limit_is_preserved_after_repeated_finish_and_refill_cycles`: somete el límite a ciclos repetidos de finalización y reposición con 25 trabajos.

### Cobertura ya existente que se conserva
- Límite máximo de ocho activas y orden FIFO.
- Inicio automático del siguiente trabajo al liberar un hueco.
- Modo paralelo y modo cola.
- Progreso individual y cálculo global.
- Retirada de trabajos en espera.
- Cambio de modo sin interrumpir trabajos ya activos.
- Pruebas TUI del diálogo, selección múltiple, ventana `Ctrl+D` y confirmación de descarga de canal.

### Alcance y límites de estas pruebas
- Las pruebas nuevas son unitarias y deterministas: no descargan archivos reales ni dependen de Internet.
- La cancelación del hilo activo sigue siendo cooperativa y la señal atómica la gestiona `App`; la prueba del gestor verifica la retirada y la reposición de la cola, no la interrupción real de una conexión.
- La recuperación se verifica en el nivel del gestor: al finalizar/retirar el trabajo fallido, la cola puede continuar. No se simula un servidor HTTP defectuoso.
- No se afirma que las pruebas hayan pasado: este entorno no tiene `cargo` ni `rustc`, por lo que no se pudieron ejecutar `cargo fmt`, `cargo test` ni `cargo build --release`.

### Verificación estática realizada
- Se añadió cobertura para 5 escenarios que faltaban en el módulo `downloads.rs`.
- Se actualizó la versión del paquete a `6.9.10-Pro` en `Cargo.toml`, `Cargo.lock` y README.
- Se conserva el código funcional del paso 8; este paso se centra en pruebas y verificación, sin alterar el comportamiento de descarga.

### Pruebas que deben ejecutarse en el equipo de desarrollo
```bash
cargo fmt --check
cargo test
cargo build --release
```

