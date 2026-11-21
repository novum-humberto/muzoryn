# MUZORYN 6.9.13-Pro — ajustes de interfaz

## Cambios

### Indicador de descargas en la barra lateral
- Se reemplazaron las barras individuales y líneas repetidas por un único indicador compacto: `⇩ activas de total · porcentaje`.
- Las barras y porcentajes individuales siguen disponibles en la ventana `Ctrl+D` / `F3`.
- Se conserva el porcentaje global agregado.

### Acceso visible a la administración de bibliotecas
- La vista Biblioteca muestra `Alt+M gestionar` en el título del panel de carpetas.
- El atajo existente sigue abriendo el navegador de directorios y no cambia su comportamiento.

### Acceso visible a la ayuda de atajos
- Se añadió `? Atajos` al menú lateral.
- Usa la acción de ayuda existente (`?` o `F1` según `keys.toml`); no se reasignaron atajos.

## Verificación
- Cambios revisados estáticamente sobre el código fuente 6.9.12-Pro.
- Pendiente ejecutar `cargo fmt --check`, `cargo test` y `cargo build --release` en un entorno con Rust/Cargo.
