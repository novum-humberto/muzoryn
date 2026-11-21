# MUZORYN 7.0.1 — Pretest 06 (correcciones 1, 4, 5, 6 y 7)

Parte del Pretest 05 (correcciones 2 y 3). Con esto quedan hechas las 7 correcciones.

## 1 — Hooks (`src/hooks.rs`, nuevo)
- **Confianza:** solo se activan si `settings.toml` es del usuario, no es escribible por grupo/otros y su carpeta tampoco (salvo sticky). Se resuelven los enlaces simbólicos antes de comprobar.
- **Cuarentena:** si no hay confianza, los hooks se desactivan y se copian a `settings.toml.hooks-en-cuarentena` (0600). Antes se borraban en memoria y el siguiente guardado los eliminaba sin aviso; ahora no se pierden y guardar no «blanquea» hooks ajenos.
- **Validación:** solo `track_changed` y `playback_stopped`; comando no vacío, ≤ 1024 caracteres, sin byte nulo; máximo 16 hooks.
- **Ejecución:** `sh -c` en su propio grupo de procesos, tiempo límite de 30 s (se mata todo el grupo) y máximo 8 simultáneos. Título/artista/álbum viajan como variables de entorno, nunca se insertan en el comando.
- **Interfaz de edición:** `muzoryn hooks [list|add|remove|restore [--yes]]`; se niega a editar si el archivo no es de confianza. Ajustes (`,`) muestra el aviso.

## 4 — Redirecciones (`media::get_checked`)
Todos los clientes HTTP usan `.redirects(0)` y siguen las redirecciones a mano (máx. 5). Se bloquea HTTPS→HTTP, los esquemas distintos de http(s) y los destinos inválidos. HTTP→HTTP y HTTP→HTTPS siguen permitidos (muchas emisoras de radio son HTTP).

## 5 — Instalador de rustup (`install.sh`)
Ya no hace `curl | sh`. Pide confirmación (o `--yes`; sin terminal y sin `--yes` se niega), descarga `rustup-init` y su `.sha256` por HTTPS/TLS 1.2, compara la suma y solo entonces lo ejecuta.

## 6 — Cierre `Shutdown`
Sin `process::exit`: el daemon detiene el audio, espera al hilo (`LocalService::shutdown_and_wait`), despierta el `accept`, borra el socket y `serve` devuelve el control a `main` (código 0).

## 7 — Snapshots
Medido en debug: serializar 1 000 pistas ≈ 8,7 ms y 10 000 ≈ 83 ms (1,3 MiB) por snapshot, cada 100 ms y por cliente. Ahora: no se reenvía un estado idéntico; la cola solo viaja cuando cambia (`queue_same`); cadencia de 100 ms al sonar y 250 ms parado/en pausa.

## Pruebas (+24 en Rust, total 178; más 10 comprobaciones de shell)
12 de hooks, 6 de redirecciones, 6 de IPC (cierre, pacer ×3, coste, cola), más `scripts/test-instalador-rustup.sh` (10 comprobaciones). Resumen por corrección: `scripts/comprobar-correcciones.sh`.
