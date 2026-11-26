# MUZORYN — Gapless pretest 01

Parte íntegramente de la 7.0.1 publicada (versión declarada sin cambios: 7.0.1, para no tocar `Cargo.lock`). **Este paquete no se presenta como compilado ni probado**: en el entorno de preparación no hay `cargo` ni `rustc`.

## Qué hace
Con el **crossfade en 0** (valor por defecto) y pistas **locales**, la pista siguiente se abre y se encadena en el mismo `Sink` de Rodio cuando faltan 1,5 s para el final de la actual. La cola de Rodio la empieza en la muestra exacta en que acaba la anterior, sin silencio. Con crossfade > 0 sigue mandando el crossfade, igual que antes. Radios y podcasts no cambian.

## Cambios
- `src/audio.rs`: el trait `AudioEngine` gana tres métodos con implementación por defecto (`supports_gapless`, `append_next`, `has_queued_next`); solo `RodioEngine` los implementa. `NullEngine` y el motor falso de pruebas no cambian de comportamiento.
- `src/core.rs`:
  - `try_gapless()` (al final de `tick`): abre el siguiente con el mismo `opener` y lo encadena.
  - `complete_gapless()`: cuando el motor deja de tener pista en espera, pasa el estado (pista actual, duración, posición, hook `track_changed`) a la nueva.
  - Si mientras tanto se quita la pista de la cola, se corta en lugar de seguir; si se reordena, se localiza por su origen.
  - `Stop`, cambiar de pista a mano y reemplazar la cola descartan la pista encadenada (cada cambio explícito crea un `Sink` nuevo).
  - 6 pruebas nuevas.

## Verificación pendiente (en tu Arch)
```bash
cargo fmt
./scripts/verificar-pretest.sh      # fmt + test --locked + clippy -D warnings
```
Pega `docs/pruebas/RESULTADOS-PRETEST-ACTUAL.txt` si algo falla.

**Prueba manual (la que de verdad importa):** crossfade 0, un álbum donde una pista enlaza con la siguiente (en directo o clásica), escuchar el cambio. Comprobar también: pausa en los últimos segundos, `n` en los últimos segundos, saltar con `→` cerca del final, shuffle y repetición (`o`).

## Riesgos conocidos
1. **Posición tras el cambio.** Según el código de Rodio 0.22 consultado, `Sink::get_pos` se reinicia con cada fuente encadenada. El proyecto fija **rodio 0.21.1**, que no pude consultar. Si tras el cambio la barra de progreso muestra un tiempo acumulado, hay que restar un desfase en `RodioEngine::position`.
2. **`Sink::len()`** se usa para detectar el cambio; existe en la API, pero no pude compilarlo.
3. **Duración inexacta** (VBR sin duración fiable): si la duración publicada es mayor que la real, la pista actual puede acabar antes de que se encadene la siguiente y hay un hueco como antes. No empeora el comportamiento previo.
4. Durante el último 1,5 s de una pista, el estado sigue en la actual hasta que la siguiente empieza de verdad.
5. Formato: no pude ejecutar `cargo fmt`; puede reordenar algunas líneas.

## Fuera de alcance: MPRIS
No incluido. Requiere añadir una dependencia (cliente D-Bus) y regenerar `Cargo.lock`, y aquí no hay red ni compilador para hacerlo ni comprobarlo.
