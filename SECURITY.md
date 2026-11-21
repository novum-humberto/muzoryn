# Política de seguridad

## Versiones con soporte
Se corrigen los fallos de seguridad en la **última versión publicada**.

## Cómo informar de una vulnerabilidad
**No publiques detalles explotables en un *issue* abierto.**

1. Usa **Security → Report a vulnerability** en la pestaña *Security* del repositorio (informe privado de GitHub), si está habilitado.
2. Si no aparece esa opción, abre un *issue* sin detalles técnicos ("quiero informar de un problema de seguridad") y la persona mantenedora te indicará cómo continuar en privado.

Incluye: versión (`muzoryn --version`), distribución, pasos para reproducirlo y el impacto que crees que tiene.

## Qué protege MUZORYN
Hooks con comprobación de permisos, redirecciones HTTP controladas, instalador con verificación SHA-256, límites en las descargas y permisos restrictivos en configuración y socket. MPRIS usa solo el bus de **sesión** de D-Bus. Detalle completo, incluida la privacidad local de MPRIS, en [docs/seguridad.md](docs/seguridad.md).
