# MUZORYN 6.9.11-Pro — revisión

## Cambios

### Límite configurable de descargas
- Nuevo ajuste persistente `parallel_downloads` (1..=8), con valor inicial 8 para conservar el comportamiento previo.
- La ventana de descargas permite reducir/aumentar el límite con ←/→.
- Ajustes → Avanzado permite modificar el mismo valor con ←/→.
- `DownloadManager` aplica el límite configurable en modo paralelo y mantiene el límite de 1 en modo cola. Al reducir el límite no se interrumpen trabajos ya activos; se espera a que terminen para iniciar nuevos hasta volver a estar dentro del límite.
- Los ajustes antiguos que no contienen `parallel_downloads` reciben el valor predeterminado y el saneamiento restringe valores fuera del intervalo.

### Indicador general
- La barra lateral muestra `activos de total` y el porcentaje global de progreso.
- El porcentaje individual continúa disponible en la ventana de descargas. El porcentaje global conserva el avance de trabajos completados durante la tanda, y las cancelaciones se descuentan del total.

### Navegación de podcasts
- Tab alterna entre la lista de podcasts suscritos y los episodios del canal seleccionado.
- Ctrl+B abre el directorio de podcasts. ←/→ alterna Populares y Categorías; `/` abre la búsqueda; Esc vuelve a Mis podcasts.
- Revisar o reproducir episodios ya no provoca que Tab salte al directorio.

## Pruebas añadidas
- Límite paralelo configurable, límites inferior/superior y reposición FIFO de trabajos.
- El progreso global no retrocede al finalizar un trabajo; la tanda siguiente reinicia su total. Cancelar elimina el trabajo del denominador.
- Valor predeterminado y saneamiento de `parallel_downloads`; compatibilidad con ajustes antiguos sin esa clave.

## Verificación pendiente
Este entorno no dispone de `cargo` ni `rustc`; por tanto no se han ejecutado `cargo fmt`, `cargo test` ni `cargo build --release`. Ejecutar esos tres comandos en un entorno Rust antes de considerar la compilación validada.
