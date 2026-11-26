# MUZORYN 7.0.1 — Pretest 03

Esta entrega parte íntegramente del paquete Pretest 02. No modifica MUZORYN 7.0.0.

## Cambios de esta entrega

1. **Atajos de podcasts visibles:** se prioriza `u · actualizar feed` en el panel de episodios y `u · recargar populares` en el directorio, para que el texto no quede oculto por el recorte del borde inferior en paneles estrechos.
2. **Navegación del directorio:** el test ahora usa `SeekForward` (flecha derecha), que es la acción que el código implementa para cambiar entre Populares y Categorías. `Tab` sigue reservado para alternar canales/episodios fuera del directorio.
3. **Indicador de descargas:** el test valida el porcentaje `50%` y el carácter temporal del indicador compacto. No exige la palabra `DESCARGAS`, porque el diseño documentado desde 6.9.13-Pro muestra un indicador compacto en la barra lateral y deja el detalle en `Ctrl+D`.
4. Se conserva la corrección de Clippy del Pretest 02.

## Estado de verificación de esta preparación

No se pudo ejecutar `cargo fmt`, `cargo test` ni `cargo clippy` en el entorno de preparación porque no está instalado Cargo/Rust. Por tanto, estos cambios todavía necesitan la verificación real en tu Arch. El informe del Pretest 02 indicó que `cargo fmt --check` detectaba formato pendiente en `src/app.rs`; ese punto no se declara corregido en esta entrega y se debe resolver ejecutando `cargo fmt` antes del verificador.

## Cómo probar

1. Descomprime el ZIP en una carpeta nueva, sin sobrescribir la 7.0.0.
2. Entra en la carpeta del proyecto.
3. Ejecuta `cargo fmt` una vez para aplicar el formato pendiente que señaló el Pretest 02.
4. Ejecuta `./verificar-pretest.sh`.
5. Pégame el contenido de `RESULTADOS-PRETEST-ACTUAL.txt`. El registro completo queda en `.pretest-logs/`.

**No ejecutes `install.sh` todavía.** Esta entrega es únicamente para pruebas de código.
