# MUZORYN 6.9.7 Pro — paso 6 hacia 7.0: diálogo de descarga paralela o en cola

## Alcance
Solo el punto 6: «Implementar el diálogo de descarga paralela o en cola».

### Hecho
- `d` con dos o más episodios marcados (tras omitir los ya descargados o en curso) abre un diálogo modal: **En paralelo** (hasta 8 a la vez) o **En cola** (una activa, el resto en orden).
- ↑/↓ cambian la opción, Enter acepta, Esc cancela (también Ayuda, Manual, Detach, Ctrl+D). Al cancelar no se añade nada y las marcas se conservan.
- Con un solo episodio marcado no se pregunta: se añade directamente con el modo actual.
- `downloads::DownloadMode` (`Parallel` límite 8, `Queue` límite 1) en `DownloadManager`: `set_mode`, `mode`, `is_full` según el modo. Cambiar a cola no interrumpe las descargas ya activas. El modo vuelve a `Parallel` cuando no queda ninguna descarga.
- Tests: `queue_mode_runs_one_at_a_time_in_order`, `parallel_is_default_and_mode_resets_when_empty`, `switching_to_queue_keeps_active_jobs_and_blocks_new_starts`, `download_dialog_opens_for_several_marked_episodes_and_cancel_keeps_marks`, `download_dialog_arrows_toggle_between_parallel_and_queue`.

### Límites conocidos
- La elección solo se pregunta al descargar marcados; `d` de un episodio suelto usa el modo vigente y `D` (canal completo) sigue con su propio grupo de hilos.
- Confirmar con Enter lanza hilos con red: no tiene test automático; probar a mano.

### Aún no hecho
Cancelación individual, recuperación de errores, conexión con podcasts (unificar `D`), prueba de las 8 simultáneas.

## Verificación
- NO compilado ni ejecutado (sin `cargo`). Pendiente: `cargo build && cargo test`.
