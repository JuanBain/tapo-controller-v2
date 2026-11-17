# T2 — QA físico RGBIC

El agente no ejecuta pruebas en las tiras. Los resultados de abajo son los que anotó el usuario.

## A. QA de consolidación — 10/10 PASS

Realizado con `npm run dev` (pruebas 1 a 9) y con el instalador de la configuración anterior, en la que Custom solo existía en desarrollo (prueba 10). Sin observaciones.

| # | Prueba | Resultado |
| --- | --- | --- |
| 1 | Custom Static, rojo/blanco/azul | PASS |
| 2 | Custom Static, verde/morado/amarillo | PASS |
| 3 | Custom Sequence, rojo/blanco/azul | PASS |
| 4 | Custom Sequence, segunda paleta | PASS |
| 5 | Rainbow Fast / Normal / Slow | PASS |
| 6 | Brillo con Rainbow Slow conserva la velocidad | PASS |
| 7 | Cambios entre Color, White, Classic, Segment y Custom | PASS |
| 8 | Independencia entre las dos tiras | PASS |
| 9 | *Restore Rainbow*, reconexión y recuperación | PASS |
| 10 | Instalador release con los controles experimentales ocultos | PASS (configuración anterior) |

## B. QA final tras la estabilización — 7/7 PASS

Comprueba los cambios de la estabilización: Custom en release, puerta por IP en la reconexión y errores visibles de Color y White. Sin observaciones. La prueba 4 es una confirmación visual del usuario; las pruebas automáticas `--release` del backend son evidencia aparte.

### Preparación

- Instalar el MSI o NSIS nuevo de `src-tauri/target/release/bundle`. Las pruebas 1 a 7 se hacen con el instalador, salvo que se indique otra cosa.
- No arrancar la app con `RUST_LOG=debug` ni `trace`.
- Parar ante un error inesperado, una desconexión que no se recupera o destellos incómodos.

| # | Prueba | Pasos | Esperado | Resultado |
| --- | --- | --- | --- | --- |
| 1 | Custom Static en release | *Effects → Custom → Static*. Elegir tres colores. *Apply Static* en la tira 1 | Se ven los tres colores. Estado «Custom Static» con las tres muestras y la latencia | PASS |
| 2 | Custom Sequence en release | *Custom → Sequence* con otra paleta. *Apply Sequence* | Colores en movimiento. Estado «Custom Sequence» | PASS |
| 3 | Rainbow Speed en release | *Classic → Rainbow*; Fast, Normal y Slow | Cambia la velocidad; el estado muestra «Rainbow · …» | PASS |
| 4 | X1–X5 inaccesibles | Abrir *Diagnostics* en el instalador release | Solo aparece *Read lighting capabilities*. No hay botones de X1–X5 ni *Restore Rainbow* de experimentos. El rechazo del backend en release lo cubre `diagnostics_follow_the_build_profile` (`cargo test --release`); es evidencia aparte y no cierra esta prueba | PASS (confirmación visual del usuario) |
| 5 | Reconexión y envío independiente | Con las dos tiras: *Reconnect* en la tira 1 mientras está en un efecto (o cerrar sesión y entrar). Después enviar un efecto a cada tira | Ambas reconectan. Cada orden afecta solo a su tira. Ninguna se queda bloqueada | PASS |
| 6 | Error de Color, White y colores rápidos | Desenchufar la tira 1 (o cortar su Wi-Fi). Con la app abierta, mover el selector de Color, el de White y un color rápido. Esperar hasta ~10 s | Aparece un mensaje de error rojo bajo el control, sin correo ni contraseña. La app no se bloquea. El efecto confirmado anterior no se borra. Volver a enchufar y usar *Reconnect* | PASS |
| 7 | Cambios rápidos | Alternar rápido brillo, efectos (Rainbow Slow, un Custom, un segmentado) y selección de tira | Cada tira acaba con lo último que se le pidió, a su brillo. Rainbow conserva Slow. No reaparece un efecto anterior | PASS |

Si una prueba falla, anotar el mensaje tal como aparece y en qué paso.
