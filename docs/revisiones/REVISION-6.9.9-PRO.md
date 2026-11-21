# MUZORYN 6.9.9 Pro — Paso 8 de 10

## Sección: conectar el gestor de descargas con el sistema de podcasts

### Implementado
- `D` ya no crea un grupo de hilos independiente: añade los episodios pendientes del canal al `DownloadManager`.
- Las descargas individuales (`d`), múltiples (`d` con marcas) y de canal completo (`D`) comparten la misma cola FIFO y el mismo límite efectivo.
- El canal completo respeta el modo actual: paralelo (hasta 8 activas) o cola (una activa).
- Los episodios ya descargados o ya presentes en el gestor se omiten para evitar duplicados.
- Cada episodio del canal aparece como trabajo individual en `Ctrl+D`, con estado, bytes y barra propia.
- La barra global contabiliza una sola vez cada trabajo del gestor.
- Cancelar la fila del canal elimina los trabajos en espera y solicita la cancelación cooperativa de los activos; también se puede cancelar un episodio individual.
- Se conserva la confirmación de doble pulsación de `D` dentro de 8 segundos.
- Los fallos de un episodio no detienen los demás; repetir `D` permite añadir los episodios que sigan pendientes.
- Actualizados `Cargo.toml`, `Cargo.lock`, README y esta nota a 6.9.9-Pro.

### Pruebas y verificación
- ZIP de entrada y estructura del proyecto inspeccionados.
- Revisión estática: `download_channel` ya no crea workers propios; `D` utiliza `DownloadManager` y `pump_downloads`.
- Se retiraron del flujo de la app los eventos exclusivos del antiguo grupo (`BulkProgress` y `BulkDone`).
- El progreso general se calcula desde la única lista de trabajos, evitando contar dos veces el lote del canal.
- No se pudo ejecutar `cargo fmt`, `cargo test` ni `cargo build --release`: `cargo`/`rustc` no están instalados en este entorno.
- La compilación y las pruebas de integración quedan pendientes de ejecución en el equipo de desarrollo.

### Pruebas manuales recomendadas
1. En Podcasts → Mis podcasts, pulsar `D` dos veces y comprobar que cada episodio aparece en `Ctrl+D`.
2. Iniciar una descarga individual y después `D`: ambas deben compartir el límite de 8.
3. Cambiar el modo a «En cola» y comprobar que solo hay una descarga activa, incluso con `D`.
4. Cancelar un episodio individual y comprobar que el resto del canal continúa.
5. Cancelar la fila del canal y comprobar que los trabajos en cola desaparecen y los activos se detienen al terminar su bloque de lectura.
6. Repetir `D`: solo deben añadirse episodios aún no descargados ni ya en curso.
