# MUZORYN 7.0-beta.3 — cambios preparados para prueba

Esta entrega contiene dos cambios implementados sobre el código fuente beta.2:

1. **Exportación de listas de radio:** el flujo ahora solicita nombre y formato en dos pasos. Formato vacío exporta M3U, M3U8, PLS, XSPF y JSON. Los archivos se guardan en `exports/radio/`; si un nombre ya existe, añade un sufijo numérico para no sobrescribirlo.
2. **Organización de descargas de podcasts:** las nuevas descargas se guardan en `podcasts/<nombre-del-podcast>/<episodio>.<ext>`. La búsqueda de episodios descargados también reconoce la estructura antigua en la raíz de `podcasts/`.

## Pendiente en esta entrega

El búfer inteligente de streaming con recuperación de conexión **todavía no está implementado**. La reproducción de podcasts conserva el comportamiento anterior. No se debe interpretar este ZIP como la beta.3 completa ni como una solución de reconexión.

## Validación

No se pudo ejecutar `cargo test` ni compilar en el entorno de preparación porque no está instalado Rust/Cargo. El código queda para revisión y prueba en el equipo Linux del usuario. No se declara compilación verificada.
