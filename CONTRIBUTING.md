# Contribuir a MUZORYN

¡Gracias por querer ayudar! Esta guía explica cómo proponer cambios.

## Antes de empezar
- Busca en los [issues](../../issues) si ya existe lo que quieres reportar o proponer.
- Para cambios grandes, abre primero un *issue* y cuéntanos la idea.

## Preparar el entorno
Requisitos: Rust estable ≥ 1.82, un compilador de C y, en Linux, `libasound2-dev` y `pkg-config` (o su equivalente en tu distribución). Detalle en [docs/instalacion.md](docs/instalacion.md) y [docs/desarrollo.md](docs/desarrollo.md).

```bash
git clone https://github.com/TU-USUARIO/muzoryn.git
cd muzoryn
cargo build
```

## Flujo de trabajo
1. Haz un *fork* y crea una rama: `git checkout -b mi-mejora`.
2. Haz tus cambios, con pruebas cuando tenga sentido.
3. Antes de abrir el PR, ejecuta:
   ```bash
   cargo fmt --all        # aplica el formato
   ./test.sh              # formato + pruebas + Clippy (-D warnings)
   ```
4. Abre un *Pull Request* explicando qué cambia y por qué. La CI debe quedar en verde.

## Criterios
- **Sin avisos de Clippy:** se ejecuta con `-D warnings`.
- **Funciones sin MPRIS:** el proyecto debe compilar también con `cargo build --no-default-features`.
- **Dependencias:** varias están fijadas con versión exacta en `Cargo.toml` (interfaz, audio, D-Bus); no las cambies sin motivo. Si cambias dependencias, haz *commit* del `Cargo.lock`.
- **Documentación:** si cambias el comportamiento visible, actualiza el `README.md`, la página correspondiente de `docs/` y el `CHANGELOG.md` (apartado «Sin publicar»).
- **Idioma:** el proyecto y su documentación están en español; el código usa nombres en inglés y comentarios en español.
- **Seguridad:** valida las URLs, rutas y entradas externas antes de usarlas, y ten en cuenta las medidas descritas en [docs/seguridad.md](docs/seguridad.md).

## Qué ayuda especialmente
- Probar el **gapless** y **MPRIS** en tu escritorio (GNOME, KDE, Hyprland, sway…) y contar el resultado: [docs/gapless.md](docs/gapless.md), [docs/mpris.md](docs/mpris.md).
- Probar el instalador en distribuciones distintas.
- Reportar emisoras o feeds que no funcionen.

## Reportar un fallo
Usa la plantilla de *issue*. Incluye tu distribución, terminal, la salida de `muzoryn --version` y los pasos para reproducirlo.

Al contribuir aceptas que tu aportación se publique bajo la licencia MIT del proyecto.
