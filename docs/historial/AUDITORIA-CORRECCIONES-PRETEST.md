# MUZORYN 7.0.0 — correcciones preliminares de seguridad

Esta carpeta es una copia de trabajo derivada del ZIP `MUZORYN-7.0.0-navegacion-compacta-legible.zip`.
El código original no se modifica. No etiquetar ni publicar este paquete como versión estable todavía.

## Cambios incluidos

1. **Socket IPC**
   - El socket creado por el daemon recibe permisos Unix `0600`.
   - Si no existe `XDG_RUNTIME_DIR`, la ruta pasa a ser `/tmp/muzoryn-<UID>/muzoryn.sock`; el directorio se crea/intenta asegurar con permisos `0700`.
   - Se limita cada línea de comando IPC a 1 MiB.
   - Se limita a 16 el número de clientes IPC atendidos simultáneamente.

2. **Configuración y hooks**
   - Al guardar `settings.toml`, el directorio de configuración se fija a `0700` y el archivo a `0600` en Unix.
   - Si el archivo de configuración tiene permiso de escritura para grupo u otros, se descartan los hooks al cargarlo. El resto de ajustes se conserva.
   - Esto reduce el riesgo de que un usuario local distinto manipule comandos configurados. No elimina el riesgo si el propio usuario configura deliberadamente un hook peligroso.

3. **Descargas**
   - Se rechaza un episodio cuyo `Content-Length` declarado supere 2 GiB.
   - También se detiene la descarga si supera 2 GiB aunque el servidor no declare el tamaño.
   - El archivo temporal `.part` incorpora un hash de la URL para evitar que dos episodios distintos con el mismo nombre temporal se pisen.

4. **Instalador**
   - Se corrige la selección de distribución en el fallback (`apt-get`, `pacman`, `dnf`).
   - Se elimina el segundo `cargo build` sin `--locked`: ahora se conserva el error real y no se actualiza silenciosamente el conjunto de dependencias.
   - Se evita interpolar `PREFIX` dentro de un `sh -c`.

5. **Identificación HTTP**
   - User-Agent actualizado a `muzoryn/7.0.0`.

## Pendiente / no afirmado como corregido

- La ejecución de hooks sigue usando `sh -c` porque es una función deliberada de la aplicación. Se protege la configuración guardada, pero debe revisarse además el modelo de permisos y la interfaz que permite editar hooks.
- No se ha implementado aún límite total de duración de descarga; se mantienen los timeouts de conexión/lectura del agente HTTP.
- No se ha resuelto todavía la colisión de nombres finales cuando dos episodios diferentes tienen exactamente el mismo título y extensión. El cambio actual evita la colisión de los archivos temporales, no la del destino final.
- No se ha cambiado el cierre `Shutdown` con `process::exit`.
- No se ha cambiado el envío de snapshots cada 100 ms ni se afirma que exista una fuga de RAM.
- No se ha cambiado la política de redirecciones HTTPS→HTTP.
- No se ha sustituido el instalador automático `curl | sh` de rustup; requiere una decisión específica de distribución/verificación.

## Pruebas

Se añadieron pruebas unitarias para aceptar una línea IPC normal y rechazar una línea superior al límite. **No se pudieron ejecutar** porque el entorno de trabajo no dispone de `cargo` ni `rustc`.

Antes de publicar: ejecutar `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings` y una prueba manual de instalación/uso en Arch Linux. Revisar también permisos reales del socket y `settings.toml` con `stat`.
