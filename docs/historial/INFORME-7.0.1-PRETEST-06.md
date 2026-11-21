# MUZORYN 7.0.1 — Informe Pretest 06

## Estado
Las 7 correcciones están hechas. Verificado aquí con Rust 1.97: formato OK, 178 tests OK (145 unitarios + 33 de interfaz), Clippy `-D warnings` OK, 10 comprobaciones del instalador OK. Las pruebas nuevas se repitieron 5 veces sin fallos. Además se probó el binario release: `muzoryn hooks` y `ctl quit` (el daemon sale con código 0 y borra el socket).

**No probado:** Arch Linux, audio real, la descarga real de rustup (el entorno no tiene red hacia static.rust-lang.org; se probó con `curl` simulado), ni HTTPS→HTTP real (la política se prueba por unidad y los redireccionamientos con servidores HTTP locales).

## Correcciones
| # | Tema | Resultado |
|---|------|-----------|
| 1 | Hooks `sh -c` | Confianza (dueño, permisos de archivo y carpeta, enlaces simbólicos), cuarentena sin pérdida de datos, validación, límite de 30 s con muerte del grupo de procesos, máx. 8 simultáneos, `muzoryn hooks list/add/remove/restore`, aviso en Ajustes |
| 2 | Límite de descarga | (Pretest 05) conexión, silencio, total y velocidad mínima |
| 3 | Nombres repetidos | (Pretest 05) sufijo `[hash]` + índice por carpeta |
| 4 | Redirecciones | Bloqueo HTTPS→HTTP, máx. 5 saltos, solo http(s); en todos los clientes |
| 5 | rustup | Sin `curl \| sh`: confirmación + descarga por HTTPS + verificación SHA-256 |
| 6 | `Shutdown` | Cierre ordenado sin `process::exit` |
| 7 | Snapshots | Sin reenvío si no cambian; cola solo cuando cambia; 100 ms sonando / 250 ms parado |

## Hallazgo extra (corregido)
Con un `settings.toml` inseguro, los hooks se borraban en memoria y el siguiente guardado los eliminaba del archivo sin avisar. Ahora van a `settings.toml.hooks-en-cuarentena` y no se reescriben como confiables.

## Medición de snapshots (debug, por cliente y por envío)
100 pistas: 13 KiB, 0,9 ms · 1 000: 132 KiB, 8,7 ms · 10 000: 1,3 MiB, 83 ms. Antes esto ocurría cada 100 ms siempre; con 10 000 pistas saturaba un núcleo. En release es unas 10 veces menos, pero no se midió.

## Cómo comprobar
```
./scripts/comprobar-correcciones.sh   # las 7 correcciones, una línea por punto
./scripts/verificar-pretest.sh        # formato + tests + clippy
```
Necesitan `libasound2-dev` y `pkg-config`.

## Decisiones mías (ajustables)
- Hooks: 30 s de límite, 8 simultáneos, 16 máximo, 1024 caracteres (`src/hooks.rs`).
- Snapshots: 100 ms / 250 ms (`SNAPSHOT_ACTIVE`/`SNAPSHOT_IDLE` en `src/ipc.rs`). Con la interfaz y el daemon parado, una orden puede tardar hasta 250 ms en reflejarse.
- Un `settings.toml` en una carpeta con permisos de grupo/otros ahora desactiva los hooks (antes solo se miraba el archivo).

## Pendiente
- Prueba manual en Arch: instalación (con y sin Rust), permisos (`stat` del socket y de `settings.toml`) y audio real.
- Cliente antiguo con daemon nuevo (o al revés): el campo `queue_same` es opcional y un cliente anterior mostraría la cola vacía en los snapshots sin cola. Conviene reiniciar el daemon al actualizar (`muzoryn ctl quit`).
- `README.md` sigue con la versión 6.9.14 en su cabecera (solo se editó la sección de hooks).
