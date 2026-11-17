# T2 — Reporte

Rama: `feature/t2-rgbic-effects` (desde `develop` @ `de9456f`). Sin commits, push ni PR. `tapo` 0.8.10. T3 no iniciado.

Detalle técnico de la librería, catálogos y matriz de parámetros: `docs/t2-capabilities.md`.

## 1. Estado actual

| Subfase | Contenido | Estado |
| --- | --- | --- |
| T2.1 | Auditoría de capacidades | Hecha |
| T2.2 | 17 efectos clásicos en la sección *Effects* | Verificada físicamente por el usuario (confirmación general) |
| T2.3 | Personalización de efectos | B1–B5, X1 y X2 PASS. X3: éxito funcional observado. X4 PASS: `transition` 3000 más lento y 750 más rápido, sin unidades ni compatibilidad general. X5: **inconcluso**. X6: consulta PASS, campos sin analizar (sección 7) |
| T2 consolidación | Velocidad de Rainbow; editor Custom Static/Sequence; estado por tira | QA físico 10/10 PASS (`t2-qa-checklist.md`, parte A) |
| T2 estabilización | Custom en release; puerta de escritura por IP; errores de Color/White; respuestas tardías | Implementada. QA final 7/7 PASS (sección 12) |
| T2.4 | 55 efectos segmentados | Verificada físicamente por el usuario (confirmación general) |
| T2.5 | Salvaguardas: detección RGBIC, validación en Rust, dispositivo explícito, `CommandGate`, latencia | Hecha; corrección de una condición de carrera en T2.3 (sección 6) |
| — | Actualización a `tapo` 0.8.10 | Hecha; regresión física R1–R9 PASS |

T2 no está cerrada: falta la salida filtrada de X6 y la autorización de cierre. QA B4 queda PASS por confirmación visual del usuario; las pruebas de backend en `--release` son evidencia aparte.

## 2. Cambios en el código

| Archivo | Cambio |
| --- | --- |
| `src-tauri/Cargo.toml`, `Cargo.lock` | `tapo` 0.8.8 → 0.8.10; línea `rsa` con `pkcs1v15` (sección 8) |
| `src-tauri/src/device.rs` | Catálogos `EFFECT_CATALOG` (17) y `SEGMENT_EFFECT_CATALOG` (55); `effect_preset`, `segment_effect_preset`, `is_rgbic_model`; `TapoDevice.model`, `supports_effects()`, `ensure_effects_supported`; `set_effect` y `set_segment_effect` con brillo opcional; `CommandGate` por dispositivo en todos los comandos de escritura; `DeviceInfo.supports_effects` |
| `src-tauri/src/commands.rs` | `device_effect` y `device_segment_effect` con `EffectRequest { name, ip, brightness? }` y `EffectResponse { success, kind, effect, brightness, elapsed_ms }`; `available_effects` devuelve `effects`, `catalog` y `segment_catalog`; `list_devices` expone `supports_effects`; los presets guardados validan el nombre de efecto; log de latencia sin datos sensibles |
| `src-tauri/src/lib.rs` | Registro de `device_segment_effect`, `device_lighting_capabilities` y `device_experiment` |
| `src-tauri/src/capabilities.rs` (nuevo, T2.3) | Filtro de la respuesta de `get_device_info_json`: devuelve solo los valores de una lista explícita de campos y los nombres (sin valores) del resto |
| `src-tauri/src/device.rs` (T2.3) | `lighting_capabilities()` (solo lectura); `Experiment` cerrado con X1–X5 (X1–X3 del ejemplo; X4 y X5 cambian un solo campo de Rainbow); `run_experiment()` a través de `CommandGate`; `experiments_enabled()` solo en builds de desarrollo |
| `src-tauri/src/commands.rs` (T2.3) | `device_lighting_capabilities(ip)` y `device_experiment({ ip, experiment })`; IP explícita obligatoria; la petición rechaza campos extra |
| `src/components/LightingDiagnostics.tsx` (nuevo, T2.3) | Sección plegable: lectura de capacidades siempre; X1–X5 con confirmación en dos pasos y *Restore Rainbow* solo si `import.meta.env.DEV` |
| `src/lib/effects.ts` | Contrato IPC tipado: `fetchEffectCatalogs`, `ConfirmedEffect`; desde T2.3, `readLightingCapabilities` y `runExperiment`. En la consolidación, `applyEffect(ip, target, brightness)` envía preset clásico (con `rainbow_speed` opcional), segmentado o Custom, y construye la confirmación desde la respuesta del backend |
| `src/components/EffectsPanel.tsx` | Subselector *Classic (17)* / *Segment (55)* / *Custom* en todas las compilaciones; indicador de envío, error real, latencia; configuración confirmada (efecto, velocidad o colores). Control *Rainbow speed* en Classic |
| `src/App.tsx` | Modos `color`, `white`, `classic`, `segment` y `custom`; último color, temperatura, efecto, pestaña, brillo, velocidad y colores de cada tira; cada pestaña reenvía su último valor (salvo Custom); el brillo reenvía el último efecto de la tira; reconexión de las tiras guardadas al iniciar sesión. Todos los envíos de efecto pasan por `sendEffect` |
| `src-tauri/src/custom_effects.rs` (nuevo, consolidación) | `CustomMode`, `EffectColor`, `RainbowSpeed`; `validate_colors`, `build_custom` (estructura X1/X2), `rainbow(speed)`. Sin restricción por compilación desde la estabilización |
| `src-tauri/src/validation.rs` | `effect_saturation` (0–100) y `effect_value` (1–100) para colores de efecto |
| `src-tauri/src/device.rs` (consolidación) | `classic_effect`, `set_effect_with_speed`, `set_custom_effect`; los tres por `CommandGate`. `set_effect` conserva su firma |
| `src-tauri/src/commands.rs` (consolidación) | `EffectRequest.rainbow_speed` opcional (solo Rainbow) y `EffectResponse.rainbow_speed` (omitido si no aplica). Comando `device_custom_effect`: IP explícita y campos cerrados. En release desde la estabilización. `device_experiment` sigue solo en desarrollo |
| `src/lib/effectState.ts` (nuevo) | Lógica pura: tipos, límites de color, velocidades, ajustes por tira y `createEffectController` (último envío por tira, reintento de brillo, descarte de respuestas superadas) |
| `src/components/CustomEffectsEditor.tsx` (nuevo) | Editor Static/Sequence de tres colores. Editar no envía; solo *Apply* |
| `tests/frontend/effectState.test.ts` (nuevo), `package.json` | 19 pruebas del estado del frontend con el runner nativo de Node 22 (`npm run test:frontend`). Sin dependencias nuevas |

Compatibilidad: los comandos y nombres de efecto existentes no cambian; los campos nuevos son opcionales. Lo recordado (color, temperatura, efectos) dura mientras la app está abierta.

## 3. Comportamiento de la UI

- **Enviando:** botón con spinner; los demás efectos quedan deshabilitados.
- **Confirmado:** solo tras una respuesta correcta de la tira; muestra la latencia.
- **Rechazado:** muestra el error y no cambia el último efecto aceptado.
- **Pestañas:** Color reenvía el último color, White la última temperatura, y Effects el último efecto del último tipo usado (Classic o Segment), en cualquier orden.
- **Brillo:** al elegir un efecto o cambiar de pestaña se envía el brillo del control deslizante. Moverlo con un efecto activo reenvía ese efecto.
- **Desconocido:** tras reiniciar la app, porque la tira no informa del efecto activo.
- **Dispositivo no compatible:** el panel de efectos se deshabilita; color, White, brillo y encendido siguen funcionando.
- **Diagnostics** (T2.3): sección plegada bajo el panel de efectos, solo en tiras compatibles. La lectura de capacidades está siempre. X1–X5, con confirmación en dos pasos y *Restore Rainbow*, solo aparecen con `npm run dev`. El instalador no los muestra, y el backend rechaza `device_experiment` en un build de distribución.
- **Rainbow speed** (consolidación): Fast, Normal y Slow en la vista Classic. Si Rainbow es el último efecto de la tira, se envía al pulsar; si no, se guarda para la próxima vez que se elija Rainbow. Se mantiene al mover el brillo y al volver a Effects. *Restore Rainbow* vuelve a Normal.
- **Custom** (consolidación; en el instalador desde la estabilización): Static o Sequence con tres colores. Abrir la vista o editar colores no envía nada; solo *Apply*. Effects no reenvía un Custom automáticamente.
- **Por tira:** pestaña, brillo, velocidad de Rainbow, colores Custom y último efecto confirmado. Seleccionar otra tira solo cambia la vista. Los brillos pendientes se llevan por tira, así que cambiar de tira no anula el de la otra.

## 4. Pruebas automatizadas (ejecutadas por el agente)

Última ejecución completa antes de este pre-cierre, en serie, después de la estabilización. La repetición de este pre-cierre está en la sección 12.

| Comando | Resultado |
| --- | --- |
| `npx tsc --noEmit -p .` | PASS |
| `npm run test:frontend` | PASS — **19/19** |
| `npm run build:frontend` | PASS |
| `cargo check` | PASS |
| `cargo clippy --all-targets` | PASS con 1 aviso preexistente de T1 (`bool_assert_comparison` en `credentials.rs:444`, no modificado). Ningún aviso de T2 |
| `cargo test` | PASS — **113** |
| `cargo test catalog` | PASS — 5 (17 clásicos y 55 segmentados sin cambios) |
| `cargo test credentials` | PASS — 19 (T1) |
| `cargo test --release build_profile` | PASS — 3: Diagnostics bloqueado en release, Custom habilitado, `experiments_enabled()` falso |
| `npm run build` | PASS — MSI y NSIS |

Evolución del número de tests de Rust: 49 (T1) → 57 (T2.2) → 68 (T2.4) → 71 (T2.3, estabilidad) → 83 (T2.3, X6 y X1) → 88 (T2.3, X2) → 89 (T2.3, X3) → 90 (T2.3, X4) → 91 (T2.3, X5) → 108 (consolidación) → 113 (estabilización). En la estabilización se eliminó `custom_effects_follow_the_build_profile`, porque Custom deja de depender del perfil de compilación. Único ajuste anterior en un test existente: `effect_response_contract` construye `EffectResponse` con `rainbow_speed: None`; su JSON esperado no cambia, porque el campo se omite cuando está vacío.

Durante el desarrollo fallaron dos ejecuciones y se corrigieron antes de la batería final:

- Rust: serde aceptaba un color escrito como lista (`[1,2,3]`). `EffectColor` ahora solo acepta un objeto con `hue`, `saturation` y `value`.
- Frontend: un reenvío de brillo reutilizaba el mismo objeto de destino, así que una respuesta antigua contaba como vigente. Cada envío tiene ahora su propia entrada.

Tests de T2, por tema:

- **Catálogos:**
  - catálogo clásico ↔ presets;
  - catálogo segmentado ↔ presets (55, ids únicos, `TapoStrip_…`, ninguno personalizado);
  - catálogo clásico sin cambios;
  - ids insensibles a mayúsculas y espacios.
- **Validación:**
  - efecto desconocido (clásico y segmentado);
  - brillo del efecto;
  - nombre de efecto en presets guardados.
- **Dispositivo:**
  - detección de modelo;
  - rechazo sin handle RGBIC;
  - rechazo en dispositivo no compatible;
  - errores de protocolo saneados.
- **Contratos IPC:**
  - `EffectRequest`;
  - `device_segment_effect`;
  - separación entre peticiones clásicas y segmentadas;
  - `EffectResponse`;
  - `available_effects`.
- **Concurrencia:**
  - `CommandGate` nunca deja más de un comando en curso;
  - atiende en orden de llegada (T2.3);
  - dos dispositivos no se bloquean entre sí.
- **Parámetros (T2.3):**
  - los presets segmentados solo envían `brightness`, `custom`, `display_colors`, `enable`, `id` y `name`;
  - en los 72 presets, el brillo es el único campo que cambia la app.
- **X6, lectura de capacidades (T2.3):**
  - los valores sensibles (IP, MAC, SSID, ids de dispositivo, hardware, firmware y OEM, nombre, región, id de efecto) nunca salen del filtro;
  - los campos permitidos se devuelven con su valor;
  - el resto aparece solo por nombre, ordenado;
  - un campo permitido con un objeto inesperado dentro se descarta;
  - una respuesta que no es un objeto da un informe vacío;
  - exige un dispositivo conectado.
- **X1 a X5 (T2.3):**
  - X1, X2 y X3 coinciden con el ejemplo oficial (salvo el `id` aleatorio);
  - X4 y X5 parten del preset Rainbow. `x4_changes_only_the_rainbow_transition` y `x5_changes_only_the_rainbow_direction` comprueban que solo cambia el campo previsto y que `transition` permanece en 1500 en X5;
  - X3 se envía con `set_segment_effect`; X1, X2, X4 y X5 con `set_lighting_effect`;
  - ids cerrados, incluidos `x5a_rainbow_direction_3` y `x5b_rainbow_direction_4`; se rechazan `x5_direction` y un campo `direction` enviado por la UI;
  - la petición exige IP y, con dos tiras, un fallo de IP no selecciona la otra;
  - se rechazan en dispositivos no RGBIC;
  - los errores de experimento se sanean;
  - los catálogos siguen en 17 y 55;
  - `experiments_enabled()` coincide con `debug_assertions`: verdadero en `cargo test` y falso en `cargo test --release`. El comando lo exige.

- **Consolidación (Rust, 17 nuevas):**
  - límites de cada componente de color, exactamente tres colores, objeto sin campos extra ni tipos incorrectos;
  - Static y Sequence con rojo, blanco y azul a brillo 100 dan el mismo JSON que X1 y X2 (salvo el `id`);
  - con otros colores y brillo solo cambian `sequence`, `display_colors` (primer color) y `brightness`; Sequence mantiene `direction` 1 y `duration` 50, Static no los lleva;
  - modos y velocidades son conjuntos cerrados;
  - Fast, Normal y Slow solo cambian `transition` (750, 1500, 3000); Fast coincide con X4-B; Normal es el preset sin cambios;
  - la velocidad se mantiene al cambiar el brillo;
  - la velocidad se rechaza en los otros 16 efectos clásicos, que siguen dando el payload del preset;
  - contratos de `rainbow_speed` y `device_custom_effect`, con rechazo de IP ausente, modos desconocidos y campos `segments`, `direction`, `duration` o `display_colors`;
  - Custom y velocidad se rechazan en dispositivos no RGBIC;
  - desde la estabilización, `custom_effects_are_enabled_in_every_build_profile` y `diagnostics_follow_the_build_profile` sustituyen a `custom_effects_follow_the_build_profile`.
- **Frontend (19, `tests/frontend/effectState.test.ts`):** validación y redondeo de colores; colores por defecto; velocidades; el brillo conserva la velocidad de Rainbow y los colores Custom; cambios rápidos de efecto, de brillo y de colores; fallos que restauran o no el destino anterior; separación entre las dos tiras; nada que reenviar tras Color o White; una respuesta tardía de Color no borra un efecto posterior; una velocidad antigua no se registra; un cambio de tira a mitad de un envío no cruza las respuestas. Prueban la lógica de `effectState.ts`; los componentes React no tienen pruebas automáticas.
- **Regresión:** los catálogos siguen en 17 y 55 y los 72 presets mantienen su payload. Los 19 tests de credenciales de T1 pasan sin cambios en `credentials.rs`, `storage.rs` ni el keyring.

## 5. Pruebas físicas (realizadas por el usuario)

Las pruebas de esta sección se hicieron con `npm run dev`. El QA posterior, incluido el del instalador, está en `t2-qa-checklist.md` y en la sección 12. Solo se marca PASS lo que el usuario confirmó.

### Efectos clásicos (T2.2), una tira

- Confirmado: los efectos se ven bien y puede cambiar rápido entre efectos y colores. Latencia inferior a 200 ms (una medición).
- Corregido durante la sesión: pasar de un efecto a Color o White no aplicaba nada; ahora cada pestaña reenvía su último valor en cualquier orden.

### Regresión con `tapo` 0.8.10, dos tiras — PASS 9/9

| # | Prueba | Resultado |
| --- | --- | --- |
| R1 | Abrir la app con Remember Me: entra sin contraseña y reconecta las dos tiras | PASS |
| R2 | Sin errores nuevos de autenticación en la terminal | PASS |
| R3 | Tira 1: encender, apagar, color, White | PASS |
| R4 | Tira 1: varios efectos clásicos | PASS |
| R5 | Tira 1: brillo con un efecto clásico activo; el efecto sigue | PASS |
| R6 | Tira 2: R3, R4 y R5 | PASS |
| R7 | Cada orden solo afecta a la tira seleccionada | PASS |
| R8 | Botón Reconnect | PASS |
| R9 | Cerrar sesión y entrar: aparecen las dos tiras sin pedir añadir dispositivo | PASS |

R9 cubre la corrección de login: `handleLogin` llama a `reconnectSavedDevices`, que reconecta cada tira guardada desconectada.

### Efectos segmentados (T2.4)

El usuario confirmó: «funcionan bien». No anotó un resultado por efecto ni por cada punto de la lista propuesta.

### Estabilidad (T2.3) — PASS 5/5

| # | Prueba | Resultado |
| --- | --- | --- |
| B1 | Brillo con un efecto segmentado activo | PASS — el brillo cambia, pero **la animación se reinicia** (limitación conocida, sección 6) |
| B2 | Mover el brillo y elegir enseguida otro efecto | PASS — queda el último efecto elegido |
| B3 | Elegir un efecto con el brillo al 20 % | PASS — los efectos respetan el 20 % |
| B4 | Cambio rápido entre los cuatro modos | PASS |
| B5 | Brillo en una tira y cambiar enseguida a la otra | PASS — la otra tira no cambia |

B2 y B3 confirman físicamente la corrección de la condición de carrera de la sección 6.

### No anotado por separado

| Prueba | Situación |
| --- | --- |
| Comparación de cada efecto con la app oficial Tapo | Sin anotar |
| Apagar y encender con un efecto activo | Sin anotar |
| Error de conexión (Wi-Fi o IP incorrecta) sin bloquear la app | Sin anotar |
| Latencia mínima y máxima con varias repeticiones | Una sola medición, &lt; 200 ms |

## 6. Revisión de estabilidad (T2.3)

Revisión del código actual, sin enviar comandos a las tiras.

| Punto | Resultado de la revisión | Evidencia |
| --- | --- | --- |
| El brillo reinicia una animación segmentada | **Sí** (B1, confirmado por el usuario). La app reenvía el efecto completo con otro `brightness` y el firmware lo arranca de nuevo. Aceptado provisionalmente como limitación conocida | Prueba B1 |
| El cambio rápido de modo conserva el último valor de cada tira | Sí. Lo recordado se guarda por IP y por tipo; las pestañas se bloquean mientras se aplica un cambio, y una respuesta tardía de un cambio anterior se descarta | `switchMode`, `modeRequest` |
| Carreras entre brillo, efectos y selección de dispositivo | **Defecto encontrado y corregido** (abajo). El brillo diferido usa la IP del momento en que se movió, así que cambiar de tira no lo redirige a la otra | `handleBrightness`, `sendEffect` |
| Remember Me y reconexión | Remember Me no cambia desde R1–R9 PASS. El riesgo de solapamiento (cada reconexión creaba otra `CommandGate`) se confirmó en la estabilización y quedó corregido con una puerta por IP (sección 11) | `reconnect_device`, `gate_for` |
| Presentación de errores | Efectos y brillo: el error del dispositivo se muestra y el estado confirmado no cambia. Color, White y colores rápidos: corregido en la estabilización; el error saneado se muestra y no borra el efecto confirmado (sección 11). QA B6 PASS | `sendColor`, `sendTemperature` |
| Efectos incompatibles sin afectar a otros controles | Sí. El backend rechaza los efectos si el modelo no es L920/L930 o el handle no es RGBIC; la UI deshabilita solo el panel de efectos | `ensure_effects_supported`, `supports_effects` |

**Defecto corregido: un brillo pendiente podía restaurar el efecto anterior.**

- **Antes:** mover el brillo programaba en 400 ms el reenvío del efecto que estaba activo *en ese momento*. Si en ese intervalo el usuario elegía otro efecto, el reenvío llegaba después y volvía a poner el anterior. Además, elegir un efecto desde el panel enviaba el brillo por defecto del preset (100 en clásicos, 50 en segmentados), no el del control deslizante.
- **Ahora:**
  - Todos los envíos de efecto (panel, pestañas, brillo) pasan por `sendEffect` en `App.tsx`.
  - `sendEffect` cancela el brillo pendiente y apunta qué efecto se pidió para esa tira; si la tira lo rechaza, vuelve al anterior.
  - El brillo diferido envía el efecto pedido más reciente.
  - Como `CommandGate` atiende en orden de llegada (test nuevo), la tira acaba con el último efecto pedido.
  - Elegir un efecto envía el brillo del control deslizante, igual que ya hacían las pestañas y el brillo.
- **Verificación:** `tsc`, build, el test de orden de `CommandGate` y las pruebas físicas B2 y B3 (PASS).

Los errores de Color, White y colores rápidos se muestran desde la estabilización (sección 11). QA B6 PASS.

### Brillo sin reiniciar los efectos segmentados: alternativas documentadas

Revisado el código de `tapo` 0.8.10. **No hay una alternativa documentada.**

| Vía | Qué hace | ¿Sirve? |
| --- | --- | --- |
| `set_brightness(b)` | Envía `set_device_info` con `brightness`. Doc: *"Pre-existing lighting effect will be removed"* | No: elimina el efecto. No se ha sustituido |
| `set().brightness(b).send()` (`ColorLightSetDeviceInfoParams`) | La misma petición `set_device_info`; la doc del builder remite a `set_lighting_effect` para efectos | No: misma semántica que `set_brightness` |
| `set_segment_effect` con otro `brightness` (actual) | Reenvía la regla `apply_segment_effect_rule` completa | Funciona, pero reinicia la animación (B1) |
| Petición específica para el brillo de un efecto activo | No existe en `tapo` 0.8.10 (`api_client.rs` solo tiene `set_lighting_effect` y `set_segment_effect` para efectos) | — |

Cualquier otra vía sería una petición no documentada; no se prueba sin autorización. Se mantiene el comportamiento actual, sin comandos repetitivos.

## 7. Pruebas experimentales

Reglas comunes:

- Una prueba por vez, primero en la tira 1. La tira 2 solo después de revisar los resultados.
- Solo valores que ya aparecen en `tapo` 0.8.10 (ejemplo oficial `examples/tapo_l930.rs` o presets).
- Un comando de backend con una lista cerrada de experimentos; la UI no puede enviar valores propios.
- Recuperación con el preset Rainbow, ya validado.
- Parar ante cualquier error, desconexión, comportamiento inesperado o luz que no vuelve con Rainbow, y no repetir en la tira 2.

### X6 — Lectura de capacidades (consulta PASS; contenido sin revisar)

- `get_device_info_json` está disponible en `tapo` 0.8.10 sin features adicionales. Las features de `tapo` son `default = []` y `python`; no existe una feature `debug`. No hubo que cambiar `Cargo.toml` ni la configuración de producción (`tauri.conf.json`, permisos).
- Petición: la misma `get_device_info` que la app ya envía al conectar. No modifica la configuración ni la luz. No pasa por `CommandGate` porque no escribe.
- **Lista de campos permitidos** (`src-tauri/src/capabilities.rs`):
  - con su valor: `model`, `type`, `hw_ver`, `fw_ver`, `device_on`, `brightness`, `color_temp`, `color_temp_range`, `hue`, `saturation`, `overheated`;
  - de un objeto `lighting_effect`, si aparece: `enable`, `custom`, `name`, `type`, `brightness`, `segments` (el `id` del efecto se excluye).
- Todo lo demás aparece **solo por nombre de clave**, sin valor, para detectar campos de segmentos sin exponer datos. Quedan excluidos, entre otros, `ip`, `mac`, `ssid`, `device_id`, `hw_id`, `fw_id`, `oem_id`, `nickname`, `avatar`, `region`, `latitude`, `longitude`, `rssi` y `default_states`.
- La respuesta no se registra en logs ni se guarda en disco; solo se muestra en la UI.
- Aviso sobre logs (preexistente, no introducido por X6): `tapo` escribe las respuestas completas del dispositivo con `debug!` y `trace!`, y el nombre de usuario en el login *passthrough*. La app usa `env_logger::init()`, que por defecto solo muestra errores, así que eso no aparece. **No arrancar la app con `RUST_LOG=debug` ni `trace`.**
- Cómo ejecutarla: *Effects → Diagnostics & experiments → Read lighting capabilities (read-only)*, con la tira seleccionada.
- **Resultado: la consulta funcionó.** El usuario confirmó que se ejecutó. El contenido filtrado todavía no se ha recibido: no se anota ningún campo, valor ni número de segmentos. No se ha vuelto a consultar las tiras.

### X1 — Colores estáticos simultáneos — PASS (usuario, una L930-5)

El dispositivo aceptó el comando. Latencia: 178 ms. Rojo, blanco y azul se vieron a la vez, en distintas partes de la tira.

Eso valida un `LightingEffect` personalizado de tipo `Static` con varios colores. *Restore Rainbow* después de X1: PASS. No se deduce el número de segmentos físicos ni el direccionamiento de LEDs: `segments [0, 1, 2]` sigue sin semántica documentada.

Configuración enviada, idéntica al ejemplo oficial (`device::Experiment::X1StaticRedWhiteBlue`):

```json
{
  "brightness": 100,
  "custom": 1,
  "display_colors": [[359, 85, 100]],
  "enable": 1,
  "id": "<uuid aleatorio de 32 caracteres, generado por tapo>",
  "name": "My Custom Static Effect",
  "type": "static",
  "expansion_strategy": 1,
  "segments": [0, 1, 2],
  "sequence": [[359, 85, 100], [0, 0, 100], [236, 72, 100]]
}
```

Colores en `[tono, saturación, valor]`: rojo `[359, 85, 100]`, blanco `[0, 0, 100]` y azul `[236, 72, 100]`.

### X2 — Secuencia personalizada — PASS (usuario, una L930-5)

El dispositivo aceptó el comando. Latencia: 139 ms. Los colores se desplazan de forma visible. *Restore Rainbow* después de X2: PASS.

Eso valida el `LightingEffect` `Sequence` del ejemplo, con `direction` 1 y `duration` 50. No se deduce qué significa `direction`, ni el número de segmentos, ni el direccionamiento de LEDs.

Configuración enviada (`device::Experiment::X2SequenceRedWhiteBlue`):

```json
{
  "brightness": 100,
  "custom": 1,
  "display_colors": [[359, 85, 100]],
  "enable": 1,
  "id": "<uuid aleatorio de 32 caracteres, generado por tapo>",
  "name": "My Custom Sequence Effect",
  "type": "sequence",
  "direction": 1,
  "duration": 50,
  "expansion_strategy": 1,
  "segments": [0, 1, 2],
  "sequence": [[359, 85, 100], [0, 0, 100], [236, 72, 100]]
}
```

`direction` y `duration` no tienen unidades ni semántica documentadas. No hay controles para cambiarlos.

### X3 — Segmento Circulating — éxito funcional observado (una L930-5)

El usuario vio movimiento: un color morado que se expande desde el inicio hasta el final de la tira. Latencia mostrada: 190 ms. *Restore Rainbow*: PASS. La segunda tira no cambió: PASS.

Queda como éxito funcional observado. No se da por cerrada la aceptación del protocolo: no hubo una confirmación aparte de la respuesta IPC. `segments: [49]` no se interpreta como el número de segmentos de la tira.

Ejemplo oficial (`device::Experiment::X3SegmentCirculating`). El constructor, con `is_custom = true`, añade `deviceType: "strip"`. Se envía con `set_segment_effect`:

```json
{
  "brightness": 100,
  "custom": 1,
  "deviceType": "strip",
  "display_colors": [[267, 56, 100, 0]],
  "enable": 1,
  "id": "<uuid aleatorio de 32 caracteres, generado por tapo>",
  "name": "My Custom Segment Effect",
  "type": "circulating",
  "segments": [49],
  "states": [[267, 56, 100, 0]]
}
```

`segments: [49]` se copia del ejemplo. No es una medición del número de segmentos del L930-5, y los cuatro números del estado no tienen semántica documentada. No hay controles para cambiarlos.

Salvaguardas de X3, X4 y X5, las mismas que X1 y X2:

- Comando `device_experiment` con IP obligatoria.
- La petición rechaza campos extra. Los ids permitidos son X1, X2, X3, X4-A, X4-B, X5-A y X5-B.
- `CommandGate` de esa tira, timeout de 10 s y errores saneados. Un fallo en la tira 1 no selecciona la tira 2.
- Exige tira RGBIC con modelo L920/L930.
- No se guarda como preset ni como escena.
- Solo en desarrollo: la UI usa `import.meta.env.DEV` y el backend exige `debug_assertions`. Comprobado también con `cargo test --release`.
- Confirmación en dos pasos. El botón muestra el nombre y la IP. *Restore Rainbow* aplica el preset clásico solo a esa IP.

### X4 — `transition` de Rainbow — PASS (una L930-5, dos valores)

En `tapo` 0.8.10 el preset Rainbow se construye con `with_transition(1500)`. `LightingEffect::with_transition` sustituye solo ese campo. El `id` sigue siendo `TapoStrip_7CC5y4lsL8pETYvmz7UOpQ`.

| | Identificador | `transition` | Resultado |
| --- | --- | --- | --- |
| Referencia | preset `rainbow` | 1500 | Base ya validada |
| X4-A | `x4a_rainbow_transition_3000` | 3000 | PASS — Rainbow más lento. *Restore Rainbow*: PASS |
| X4-B | `x4b_rainbow_transition_750` | 750 | PASS — Rainbow más rápido. *Restore Rainbow*: PASS |

La segunda tira no cambió. Dentro de estos dos valores, `transition` modifica la velocidad observable de Rainbow en esta L930-5. No hay unidades documentadas y no se extiende el resultado a otros efectos ni a otros modelos.

### X5 — `direction` de Rainbow — INCONCLUSO

Resultado del usuario, en la tira 1:

| | `direction` | Observación |
| --- | --- | --- |
| X5-A | 3 | Movimiento observado |
| X5-B | 4 | Mismo sentido de movimiento que X5-A |

- La segunda tira no cambió.
- No se confirmó la comparación con `direction` 1 (Rainbow original).
- No se confirmaron por separado la aceptación IPC ni *Restore Rainbow* de cada uno.

No se afirma que 3 o 4 inviertan el sentido ni que cambien algo frente a 1. No hay selector de dirección. X5-A y X5-B siguen como experimentos de desarrollo.

Configuración enviada (sin cambios desde su preparación):

`LightingEffect::with_direction` es público y sustituye solo `direction`. Rainbow queda con `direction` 1 y `transition` 1500; X5 no toca `transition`. El `id` del preset se conserva. Los valores 3 y 4 son los que ya usan Ocean y Aurora/Icicle. No tienen significado documentado.

| | Identificador | `direction` | `transition` |
| --- | --- | --- | --- |
| Referencia | preset `rainbow` | 1 | 1500 |
| X5-A | `x5a_rainbow_direction_3` | 3 | 1500 |
| X5-B | `x5b_rainbow_direction_4` | 4 | 1500 |

El test `x5_changes_only_the_rainbow_direction` exige que la única diferencia sea `direction`.

Para cerrarlo haría falta comparar, en la misma sesión, Rainbow original (`direction` 1) con X5-A y X5-B, anotando la respuesta de cada envío y *Restore Rainbow*. No está autorizado como siguiente paso.

## 8. Historial técnico: actualización a `tapo` 0.8.10

Primer intento de segmentos (detenido por incompatibilidad):

| Paso | Resultado |
| --- | --- |
| Versión inicial en `Cargo.lock` | `tapo` 0.8.8 (manifiesto `tapo = "0.8"`) |
| `tapo` 0.8.10 tal cual | **FAIL** — `error[E0432]: unresolved import rsa::Pkcs1v15Encrypt` dentro de `tapo` |
| Fijar `rsa` 0.10.0-rc.17 | **FAIL** — 9 errores de compilación en `rsa` |
| `tapo` 0.8.10 + línea `rsa` con `pkcs1v15` | `cargo check` y `cargo test` (57) PASS sin tocar el código del proyecto |

El usuario aprobó la opción A:

| | Antes | Después |
| --- | --- | --- |
| `tapo` | 0.8.8 (`tapo = "0.8"`) | 0.8.10 (`tapo = "0.8.10"`) |
| `rsa` | 0.9.10, transitiva | 0.10.0; línea directa con `pkcs1v15` |

- Necesaria y suficiente: sin la línea `rsa` falla un solo import; con ella compila sin otros cambios.
- Lockfile mínimo: `cargo update -p tapo --precise 0.8.10` desde el de `develop`. Un primer intento subía también `tokio`, `serde_json` y otros, y se rehízo.
- Credenciales: `credentials.rs`, `storage.rs` y el keyring no cambian. Los 19 tests del gestor de credenciales siguen pasando. La autenticación real quedó cubierta por R1, R2, R8 y R9.

## 9. Siguiente decisión

- QA B4 registrado como PASS por confirmación visual del usuario. Las pruebas `diagnostics_follow_the_build_profile` y `experiments_follow_the_build_profile` en `--release` son evidencia aparte.
- Pegar la salida filtrada de X6 (campos con valor y nombres de claves sin valor). La consulta pasó; el contenido filtrado no se ha recibido y no se documenta ningún campo del dispositivo.
- X5 queda inconcluso; no se propone ningún selector de dirección.
- PR hacia `develop`: no realizado; solo con autorización.

## 10. Consolidación de la personalización

### Estado por categoría

| Categoría | Contenido |
| --- | --- |
| Validado físicamente | Encendido, color, White, brillo (2 tiras); 17 clásicos y 55 segmentados (confirmación general); B1–B5; X1 (178 ms); X2 (139 ms); X4 (750 más rápido, 1500 original, 3000 más lento); consulta X6 (ejecución, sin contenido); QA A 10/10; QA B 1, 2, 3, 5, 6 y 7 en el instalador release |
| Observado, sin confirmación completa | X3 (morado que se expande, 190 ms; éxito visual, confirmación IPC independiente pendiente); X5 (movimiento con 3 y con 4, mismo sentido; sin comparación con 1) |
| Implementado y confirmado en el instalador | Ocultar X1–X5 en *Diagnostics* del instalador (QA B4 PASS, confirmación visual del usuario). El backend en release también los rechaza, cubierto por pruebas automáticas |
| Experimental | X1–X5 en *Diagnostics*, solo con `npm run dev`. Siguen en el código, restringidos a desarrollo |
| No soportado o no verificado | Control por LED; número de segmentos del L930-5; significado de `segments [0, 1, 2]` y `[49]`; `direction`; velocidad de otros efectos; leer el efecto activo; campos devueltos por X6 |

### Diseño

- **Backend como fuente de verdad.** El payload se construye en Rust (`custom_effects.rs`) a partir de tipos públicos de `tapo` 0.8.10 (`LightingEffect::new`, `with_sequence`, `with_segments`, `with_expansion_strategy`, `with_direction`, `with_duration`, `with_transition`). La UI solo envía modo, tres colores y brillo, o un nombre de preset y una velocidad cerrada. No hay JSON libre.
- **Coherencia de colores.** `sequence` contiene los tres colores validados y `display_colors` el primero, igual que el ejemplo oficial y X1/X2. El backend devuelve los colores que envió y la UI muestra esos, no los pedidos.
- **Release.** Desde la estabilización, Custom Static/Sequence y la velocidad de Rainbow están en todas las compilaciones. X1–X5 siguen solo en desarrollo: la UI usa `import.meta.env.DEV` y `device_experiment` comprueba `experiments_enabled()` antes de buscar la tira. X6 permanece en release como consulta filtrada de solo lectura.
- **Concurrencia.** Cada envío pasa por `CommandGate` de su tira. En la UI, `createEffectController` guarda el último envío de cada tira; el brillo diferido reenvía ese destino con su velocidad o sus colores, y una respuesta de un envío ya superado no cambia el estado confirmado.
- **Sin comandos automáticos.** Abrir Custom, editar colores o elegir una velocidad sin Rainbow activo no envía nada.
- **Sin persistencia.** Velocidad, colores, pestaña y brillo por tira duran mientras la app está abierta. No hay escenas ni presets nuevos. Remember Me y el gestor de credenciales no cambian.

### Limitaciones

- Los tres colores no se asignan a zonas físicas conocidas. No se conoce el direccionamiento individual de LEDs.
- En Sequence no hay control de velocidad.
- Un brillo sobre un efecto segmentado sigue reiniciando la animación (B1).
- Al pulsar una velocidad sin Rainbow activo no aparece un aviso propio; la selección queda marcada y se usa al elegir Rainbow.
- X3 tuvo éxito visual; falta la confirmación IPC independiente.
- X5 sigue inconcluso.
- El contenido filtrado de X6 no se ha recibido.

## 11. Estabilización

### Custom en producción

- **UI:** la vista *Custom* no depende de `import.meta.env.DEV`. *Diagnostics* conserva `EXPERIMENTS_ENABLED = import.meta.env.DEV`: en release solo muestra *Read lighting capabilities*.
- **Backend:** `device_custom_effect` no comprueba el perfil de compilación. Se eliminó `custom_effects_enabled()`. Conserva la petición cerrada, tres colores validados en Rust, Static y Sequence, la IP explícita, `CommandGate` y la confirmación construida desde la respuesta.
- **Diagnostics:** `device_experiment` comprueba `experiments_enabled()` (`debug_assertions`) antes de buscar el dispositivo. No se activó ninguna función experimental de forma global y no se eliminaron los experimentos.
- **Pruebas en debug y en `--release`:** `custom_effects_are_enabled_in_every_build_profile` (Custom llega a buscar la tira y rechaza entradas inválidas), `diagnostics_follow_the_build_profile` (en release: «Lighting experiments are only available in development builds») y `experiments_follow_the_build_profile`.
- **X6** sigue en todas las compilaciones: solo lectura, filtrada, IP explícita, sin log del contenido.

### Defectos corregidos

| # | Defecto | Corrección |
| --- | --- | --- |
| A | Los errores de Color, White y colores rápidos no se mostraban | `sendColor` y `sendTemperature` muestran el error saneado bajo el control, solo para la tira que falló. Un fallo no borra el efecto confirmado. QA B6 PASS |
| B | Cada reconexión creaba otra `CommandGate`, así que una escritura en curso podía solaparse con la siguiente sobre la misma tira | `gate_for(ip)`: una puerta por IP para todo el proceso. Tiras distintas siguen independientes. QA B5 PASS |
| C | Respuestas tardías de Color/White, de velocidad de Rainbow, de `device_info` y errores de brillo de otra tira | El efecto solo se borra si no hubo un envío posterior; la velocidad solo se guarda si coincide con el último Rainbow pedido; se ignora `device_info` si la selección cambió; los errores van etiquetados por IP. QA B7 PASS |

Clippy de T2: ningún aviso. Permanece `bool_assert_comparison` en `credentials.rs:444` (T1), sin modificar ese archivo.

### Riesgos que quedaron tras la estabilización

- La puerta solo cubre escrituras. El login de una reconexión puede coincidir con una escritura de la conexión anterior.
- La puerta se identifica por IP. Si el router cambia la IP, la tira se trata como otro dispositivo.
- Un error de cambio de modo de una tira puede verse un momento al seleccionar la otra.
- Los componentes React no tienen pruebas automáticas; la lógica de `effectState.ts` sí.
- El brillo sobre un efecto segmentado reinicia la animación (B1).
- X5 sigue inconcluso. X6 sigue sin contenido revisado.

## 12. Pre-cierre

### QA

Parte A: 10/10 PASS, sin observaciones. La prueba 10 se hizo con el instalador anterior, cuando Custom aún no estaba en release.

Parte B, con el instalador de la estabilización, sin observaciones:

| # | Prueba | Resultado |
| --- | --- | --- |
| 1 | Custom Static desde el instalador release | PASS |
| 2 | Custom Sequence desde el instalador release | PASS |
| 3 | Rainbow Fast / Normal / Slow desde release | PASS |
| 4 | Diagnostics X1–X5 ocultos en release | PASS (confirmación visual del usuario) |
| 5 | Reconexión e independencia de ambas tiras | PASS |
| 6 | Errores de Color, White y colores rápidos | PASS |
| 7 | Cambios rápidos de brillo, efectos y dispositivo | PASS |

B4 es PASS por confirmación visual del usuario. Que el backend rechace `device_experiment` en `--release` es evidencia automática separada.

### X6

La consulta se ejecutó y fue aceptada. El contenido filtrado todavía no se ha recibido. No se anota ningún campo, valor ni número de segmentos del dispositivo. No se ha vuelto a consultar las tiras en este pre-cierre.

### Verificación automática de este pre-cierre

Repetida en serie, sin cambiar código. `npm run dev` seguía abierto, así que Cargo usó `src-tauri/target/precierre` (ignorado por Git) para no tocar ese proceso. El aviso de Clippy es el de T1 en `credentials.rs:444`.

| Comando | Resultado |
| --- | --- |
| `npx tsc --noEmit -p .` | PASS |
| `npm run test:frontend` | PASS — 19/19 |
| `npm run build:frontend` | PASS |
| `cargo check` | PASS |
| `cargo clippy --all-targets` | PASS, 1 aviso de T1 |
| `cargo test` | PASS — 113 |
| `cargo test --release build_profile` | PASS — 3 |
| `npm run build` | PASS — MSI y NSIS en `src-tauri/target/precierre/release/bundle` |

Los 17 clásicos y los 55 segmentados siguen cubiertos por `classic_catalog_is_unchanged` y `segment_catalog_matches_the_library_presets`. Los 19 tests de credenciales de T1 pasan. `credentials.rs` y `storage.rs` no están en el diff.

### Bloqueantes para cerrar T2

1. La salida filtrada de X6 no se ha recibido. La consulta está hecha; el contenido no.
2. Autorización explícita de cierre de T2. El commit y push de la rama se autorizaron aparte; el PR no.
