# MUZORYN 7.0.1 — Pretest 04

Parte del Pretest 03 (con `cargo fmt` aplicado a todo el proyecto).

## Cambios
1. **`d` no repite episodios:** si el episodio ya está descargado o ya está en la cola, avisa y no lo vuelve a descargar.
2. **Reintentos:** al abrir la conexión de un episodio se reintentan hasta 3 veces (esperas de 1 s y 2 s) los fallos de red, HTTP 429 y 5xx. Esto afecta a descargas en paralelo y al canal completo (`D`), donde varios servidores rechazan conexiones simultáneas. Respeta la cancelación.
3. **Mensaje de error más claro:** `Descarga fallida: el servidor respondió HTTP <código>`.

## Pendiente
Causa raíz del fallo en descargas múltiples no confirmada: depende del mensaje exacto de error (línea de estado `Falló «…»: …`). No se encontró en el código ningún sufijo « 1» en los nombres.
