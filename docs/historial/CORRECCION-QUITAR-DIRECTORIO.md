MUZORYN 7.0.0-beta.3 — Corrección de quitar directorio

Cambio realizado:
- La tecla x en el selector de carpetas de música comprueba primero la carpeta resaltada.
- Si la carpeta resaltada no es una raíz registrada, comprueba la carpeta actual para conservar el comportamiento anterior.
- La comparación de rutas tolera rutas equivalentes, componentes .. y enlaces simbólicos cuando pueden resolverse.
- Al quitar una raíz se retiran del índice sus pistas, favoritos y referencias de listas; no se borran archivos ni carpetas del disco.
- Se actualizó la ayuda de teclado y se añadieron pruebas unitarias para comparación de rutas.

Verificación del paquete:
- ZIP comprobado con unzip -t.
- No fue posible compilar ni ejecutar cargo test en este entorno: no están instalados cargo ni rustc.
