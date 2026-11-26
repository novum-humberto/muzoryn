# MUZORYN 7.0.1 — gapless + MPRIS pretest 03 (sin compilar ni probar aquí)

## Hallazgos del pretest 02
- `Cargo.lock` sin `zbus` → `--locked` aborta antes de ejecutar ningún test.
- `cargo fmt --check` falla en `src/mpris.rs`.
- `mpris.rs` nunca compilado (API zbus 4.4 escrita de memoria).
- Parpadeo al cambiar de pista/capítulo.

## Cambios
- `scripts/preparar-lock.sh` (nuevo): `cargo fetch` sin `--locked` para añadir zbus a `Cargo.lock`. `verificar-pretest.sh` lo llama solo si falta zbus. Requiere red.
- `src/mpris.rs`: `let … else` reformateados a mano; `verificar-pretest.sh` ahora ejecuta `cargo fmt` antes de `--check`.
- `src/app.rs`: `now_cover` conserva la carátula anterior mientras carga la nueva y precarga la de la siguiente pista (antes: hueco → Kitty borraba imágenes / iTerm y Sixel limpiaban pantalla).
- `src/core.rs`: `begin_open` no muestra «Conectando…» ni borra la duración si ya suena una pista normal (episodios por URL). Prueba nueva.
- Scripts: `verificar-pretest.sh` y `comprobar-correcciones.sh` incluyen `--features mpris`, pruebas de gapless y del podcast.

## Pasos
1. `./scripts/preparar-lock.sh` (con red).  2. `./scripts/verificar-pretest.sh`.  3. `./scripts/comprobar-correcciones.sh`.
4. Manual: `muzoryn ctl quit`; `MUZORYN_DEBUG_GAPLESS=1 muzoryn`; crossfade 0; revisar `/tmp/muzoryn-gapless.log` («encadenada», «cambio gapless completado», sin «SIN encadenar»).
5. Parpadeo: cambiar pista y capítulo en Kitty/iTerm/Sixel/bloques.
6. MPRIS: `muzoryn --daemon &`; `playerctl -p muzoryn status|metadata|play-pause|next|position 30|volume 0.5`.

## Sin resolver
- Posible desfase de `Sink::get_pos` y comportamiento de `Sink::len()` en rodio 0.21.1.
- Gapless real en podcasts por URL (no encadenan; solo se evitó el parpadeo de estado).
- Errores de compilación de `mpris.rs` que aparezcan al compilar por primera vez.
