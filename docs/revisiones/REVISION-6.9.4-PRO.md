# MUZORYN 6.9.4 Pro — paso 3 hacia 7.0: selección de episodios

## Alcance
Solo el punto 3: «Implementar la selección de episodios».

### Hecho
- Podcasts → panel Episodios: `m` marca/desmarca el episodio actual (se ve `✓` y «N marcados» en el título del panel); `M` quita todas las marcas.
- Las marcas se guardan por URL del episodio (`App::episode_marks`), así que sobreviven al mover el cursor.
- `d` con episodios marcados los añade todos al gestor (omite los ya descargados o ya en curso) y respeta el máximo de 8 activas; el resto queda en cola (paso 2). Sin marcas, `d` funciona como antes.
- Ayuda, manual y etiqueta de la acción actualizados; test `episode_marks_toggle_count_and_clear`.

### Aún no hecho
Marcar todos de una vez, subventana, barras individuales/global, diálogo paralela/cola, cancelación individual, recuperación de errores.

## Verificación
- NO compilado ni ejecutado (sin `cargo` en el entorno de preparación). Pendiente: `cargo build && cargo test`.
- `d` con marcas no tiene test automático (lanzaría hilos con red); comprobar a mano.
