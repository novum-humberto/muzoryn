# 🔗 Reproducción sin cortes (gapless)

El *gapless* encadena una pista con la siguiente **sin silencio entre ambas**. Es útil en álbumes en directo, música clásica y discos conceptuales donde una canción enlaza con la siguiente.

> 🧪 **Estado: experimental.** Está pendiente de validación auditiva en Linux. Si notas cortes o comportamientos raros, desactívalo subiendo el crossfade (ver abajo) y abre un *issue*.

## Cuándo se activa
Se activa solo cuando se cumplen **todas** estas condiciones:

| Condición | Detalle |
|---|---|
| **Crossfade = 0** | Es el valor por defecto. Con crossfade > 0 manda el crossfade, como siempre |
| **Pistas locales** | La actual y la siguiente deben ser archivos de tu biblioteca |
| **Hay siguiente** | Respeta cola, aleatorio (`s`) y repetición (`o`); tras la última pista sin repetición no encadena |
| **Duración conocida** | Si la duración publicada del archivo no es fiable (algunos VBR), puede haber un hueco como antes |

Las **emisoras de radio** y los **podcasts** no cambian: no se encadenan.

## Cómo funciona
Cuando faltan **1,5 s** para el final de la pista actual, MUZORYN abre la siguiente y la añade a la cola del mismo `Sink` de Rodio, que la empieza en la muestra exacta en que acaba la anterior. Cuando la siguiente empieza a sonar, el estado (pista actual, duración, posición y el hook `track_changed`) pasa a ella.

Cualquier acción **explícita** descarta la pista ya encadenada y vuelve al camino normal: parar, cambiar de pista a mano o reemplazar la cola. Si quitas de la cola la pista ya encadenada, no se pasa a ella.

## Desactivarlo
Pon un crossfade mayor que 0: en la vista **Audio y EQ** (<kbd>0</kbd>) o con `crossfade_secs` en `settings.toml` (0–12 s; consulta [Configuración](configuracion.md)).

## Diagnóstico
Si el cambio de pista no suena continuo, registra lo que ocurre:

```bash
muzoryn ctl quit                      # un daemon viejo sigue con el binario antiguo
MUZORYN_DEBUG_GAPLESS=1 muzoryn       # arranca de nuevo con el registro activado
tail -f /tmp/muzoryn-gapless.log
```

Mensajes que verás en `/tmp/muzoryn-gapless.log`:

| Mensaje | Significado |
|---|---|
| `intento: pos=… dur=…` | Se acerca el final y se intenta encadenar |
| `encadenada #N (título)` | La siguiente quedó añadida al motor |
| `cambio gapless completado -> #N` | La siguiente ya suena y el estado pasó a ella ✅ |
| `pista terminada SIN encadenar` | No se pudo encadenar: hubo hueco |
| `la siguiente no es archivo local`, `no se pudo abrir la siguiente`, `append_next falló` | Causa del fallo |

Al abrir un *issue*, adjunta ese archivo.

## Limitaciones conocidas
- Con la versión de Rodio fijada (0.21.1) no se ha podido confirmar que el tiempo transcurrido se reinicie bien tras cada cambio. Si la barra de progreso muestra un tiempo acumulado tras el cambio, es un fallo conocido por comprobar.
- Durante el último 1,5 s de una pista, el estado sigue mostrando la pista actual hasta que la siguiente empieza de verdad.
- Si la duración publicada es mayor que la real, la pista puede acabar antes de encadenar y hay un hueco, igual que sin *gapless*.
