# MUZORYN 6.9.8 Pro — Paso 7 de 10

## Sección: cancelación de descargas y recuperación de errores

### Implementado
- `x` en la ventana `Ctrl+D` solicita cancelar la descarga seleccionada.
- Las descargas en cola se eliminan inmediatamente sin afectar las activas.
- Las descargas activas usan un token atómico y se detienen cooperativamente entre bloques de lectura.
- El archivo temporal `.part` se elimina cuando se cancela o falla una descarga.
- El hueco de una descarga activa no se reutiliza hasta que el hilo confirma su terminación; así se conserva el límite real de concurrencia.
- La descarga de canal completo puede cancelarse desde su fila con `x` (equivalente a solicitar la cancelación del canal).
- Los errores muestran el episodio y el motivo; los fallos no detienen el resto de la cola.
- La ventana muestra la fila seleccionada y la ayuda contextual de cancelación.
- Se añadió una prueba unitaria para retirar trabajos en cola.

### Límites conocidos
- La cancelación activa es cooperativa: se comprueba entre bloques de lectura. Si la conexión está bloqueada esperando datos, la respuesta depende del timeout de la biblioteca HTTP.
- No se conserva un historial permanente de trabajos cancelados o fallidos; se informa mediante notificación/toast y se retira el trabajo del gestor.
- La cancelación de canal completo conserva su mecanismo anterior basado en `AtomicBool`.

### Verificación
- Revisión estática del cambio realizada.
- Compilación y tests no ejecutados en este entorno: `cargo` no está instalado.

### Pruebas recomendadas en el equipo de desarrollo
```bash
cargo fmt -- --check
cargo test
cargo build --release
```

Abrir `Ctrl+D`, seleccionar una descarga y pulsar `x`. Repetir con una entrada en cola y con una descarga activa; comprobar que desaparece el `.part` y que la siguiente descarga solo arranca cuando la activa confirma su parada.
