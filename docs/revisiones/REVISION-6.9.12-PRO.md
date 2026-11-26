# MUZORYN 6.9.12-Pro — bibliotecas configurables

## Cambios

### Selección en el primer inicio
- Si no existe `settings.toml`, MUZORYN abre un navegador de directorios antes del primer escaneo.
- No se presupone que la música esté en `~/Music`; el usuario elige la carpeta real.
- El usuario puede navegar desde su directorio personal hasta `/media`, `/mnt` o `/run/media` para localizar unidades externas.

### Gestión desde la interfaz
- Nuevo atajo `Alt+M` para abrir el navegador de bibliotecas desde cualquier vista y regresar a Biblioteca.
- `Enter` entra en carpetas; `s` añade la carpeta actual a `music_dirs`; `x` quita la raíz actual; `Backspace` sube un nivel; `Esc` cierra el navegador.
- Los cambios se guardan en `settings.toml` y al añadir una carpeta se inicia un escaneo incremental.
- El escaneo no elimina del índice las pistas bajo una raíz que no devolvió archivos; así se evita perder el índice de una unidad externa desconectada o vacía.
- La configuración manual de `music_dirs` sigue siendo compatible.

## Verificación pendiente
Este entorno no dispone de `cargo` ni `rustc`. No se han ejecutado `cargo fmt`, `cargo test` ni `cargo build --release`. Deben ejecutarse antes de declarar la versión 7.0 compilada y validada.
