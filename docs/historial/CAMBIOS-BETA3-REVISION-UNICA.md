MUZORYN 7.0.0-beta.3 — REVISIÓN ÚNICA

Este paquete reúne los cambios para que se puedan revisar juntos:

1. Podcasts: reproducción en streaming con búfer progresivo en memoria (canal acotado de 512 bloques de 8 KiB, aproximadamente 4 MiB). Si se interrumpe la conexión, conserva los datos ya recibidos y trata de reanudar desde el byte pendiente mediante HTTP Range cuando el servidor lo permite. No descarga el episodio completo antes de reproducir.
2. Podcasts: barra de progreso basada en la duración publicada en el RSS (itunes:duration). Si el feed no publica duración, no puede mostrarse un porcentaje fiable.
3. Biblioteca musical: en el selector de carpetas, s añade el directorio actual y x retira de la biblioteca el directorio raíz actual. Al retirarlo, MUZORYN elimina del índice las pistas de esa ruta, así como sus favoritos y referencias en listas; NO borra las carpetas ni los archivos físicos del disco.
4. Se conservan los cambios previos de exportación de listas de radio y organización de descargas de podcasts por canal.

Límites conocidos:
- La duración del búfer depende del bitrate. Aproximadamente: 128 kbps = 4 min 22 s; 256 kbps = 2 min 11 s; 320 kbps = 1 min 46 s.
- La reanudación desde el byte pendiente depende de que el servidor admita peticiones HTTP Range y entregue Content-Range válido.
- Si el RSS no contiene itunes:duration y el decodificador no puede determinar la duración del flujo, la barra no puede calcular el porcentaje.

Verificación de entrega:
- Se revisó la estructura del ZIP con unzip -t.
- Se añadió una prueba unitaria para comprobar que retirar una raíz no elimina archivos físicos y no afecta a rutas vecinas con nombres parecidos.
- No se pudo compilar ni ejecutar cargo test en este entorno porque no hay cargo/rustc instalados. La compilación y pruebas reales quedan pendientes en Linux.
