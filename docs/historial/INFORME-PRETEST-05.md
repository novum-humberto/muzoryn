# MUZORYN 7.0.1 — Informe Pretest 05

## Verificación (ejecutada aquí, Rust 1.97)
`./scripts/verificar-pretest.sh`: FORMATO OK · PRUEBAS OK (121 unitarias + 33 TUI = 154) · CLIPPY `-D warnings` OK.
Pruebas de red de descargas repetidas 5 veces sin fallos. No se probó en Arch Linux ni con audio real.

## Logrado
| # | Pendiente | Estado |
|---|-----------|--------|
| 2 | Límite de tiempo en descargas | **Hecho** (conexión, silencio, total, velocidad mínima; limpia `.part`) |
| 3 | Colisión de nombres | **Hecho** (sufijo `[hash]`, índice por carpeta, candado, dedupe por URL) |
| — | Orden del proyecto | **Hecho**: docs en `docs/{revisiones,historial,pruebas}`, script en `scripts/`, raíz limpia |
| — | Versión | 7.0.1 en Cargo.toml/Cargo.lock/User-Agent |

## Falta (no tocado, a petición)
1. **Hooks `sh -c`:** revisar modelo de permisos e interfaz de edición (hoy solo se descartan si `settings.toml` es escribible por otros).
4. **Redirecciones HTTPS→HTTP:** ureq las sigue; falta bloquearlas o avisar.
5. **Instalador `curl | sh` de rustup:** pedir confirmación o verificar.
6. **`Shutdown` con `process::exit`:** cierre ordenado.
7. **Snapshots cada 100 ms:** medir consumo.
Además: prueba manual de instalación y permisos (`stat` del socket y `settings.toml`) en Arch.

## Observaciones
- Los tiempos por defecto (2 h, 60 s/16 KiB) son decisión mía; ajustables en `DownloadLimits::default()`.
- `README.md` sigue mostrando versión 6.9.14; no se modificó.
