MUZORYN 7.0-beta.3 — búfer y progreso de podcasts

- Los podcasts comienzan a reproducirse como flujo; no se descarga el episodio completo antes de empezar.
- Búfer progresivo en memoria de aproximadamente 4 MiB (512 bloques de 8 KiB).
- Si se interrumpe la conexión, intenta reconectar y continuar desde el byte recibido mediante HTTP Range, si el servidor lo admite.
- La radio conserva su búfer anterior.
- La barra de progreso de podcasts utiliza la duración publicada por el feed RSS (itunes:duration), cuando está disponible. Acepta MM:SS, HH:MM:SS o segundos. Si el feed no publica duración, no es posible mostrar un porcentaje/tiempo total fiable.
- La duración RSS se usa para mostrar el progreso; no convierte el flujo en una descarga previa ni habilita necesariamente el salto dentro del flujo.

Verificación: el ZIP se comprueba estructuralmente. No se pudo compilar ni ejecutar pruebas Rust en este entorno porque no hay cargo/rustc instalados.
