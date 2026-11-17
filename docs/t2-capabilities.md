# T2 — Capacidades RGBIC (Tapo L930-5)

Capacidades RGBIC del fork, según el código del checkout y el código fuente de la dependencia `tapo` resuelta en `Cargo.lock`.

- Versión efectiva de `tapo`: **0.8.10** (antes 0.8.8). Ver «Actualización a 0.8.10».
- `set_segment_effect` existe desde **`tapo` 0.8.9** (2026-02-21), según el `CHANGELOG.md` del paquete. La 0.8.10 (2026-02-22) solo corrige `HubHandler::s200d`. Una auditoría anterior lo atribuía a 0.13.0: **corregido**.
- Hardware objetivo: 2 × L930-5 RGBICW (5 m).
- Fuentes revisadas en `tapo` 0.8.10: `src/requests/set_device_info/lighting_effect.rs`, `src/requests/set_device_info/segment_effect.rs`, `src/api/rgbic_light_strip_handler.rs`, `src/responses/device_info_result/rgbic_light_strip.rs`, `examples/tapo_l930.rs`, `README.md` y `CHANGELOG.md`.

## Categorías

| Categoría | Significado |
| --- | --- |
| Verificada físicamente | Integrada en la app y confirmada por el usuario en sus tiras |
| Implementada, sin prueba física anotada | Integrada y cubierta por pruebas automatizadas; el usuario no la confirmó por separado |
| Documentada por la dependencia | `tapo` la ofrece con documentación o un ejemplo, pero no se ha probado en el L930-5 |
| Experimental | Campo existente sin unidades, rangos ni semántica documentados |
| No soportada | No existe mecanismo documentado |

## Matriz de capacidades

| Capacidad | Fuente | Categoría | Notas |
| --- | --- | --- | --- |
| Encendido/apagado, color HSV, temperatura 2500–6500 K, brillo 1–100 | `RgbicLightStripHandler::{on, off, set_hue_saturation, set_color_temperature, set_brightness}` | Verificada físicamente (2 tiras) | T1 con una tira; regresión R3, R6 y R7 con las dos (`t2-report.md`) |
| 17 efectos clásicos (`LightingEffectPreset`) | `set_lighting_effect` | Verificada físicamente (confirmación general) | Lista abajo. Sin resultado por efecto |
| 55 efectos segmentados (`SegmentEffectPreset`) | `set_segment_effect` → `apply_segment_effect_rule` | Verificada físicamente (confirmación general) | El usuario confirmó que funcionan bien. Sin resultado por efecto |
| Brillo de un efecto clásico activo | `LightingEffect::with_brightness` | Verificada físicamente | R5: el efecto sigue y cambia la intensidad |
| Brillo de un efecto segmentado activo | `SegmentEffect::with_brightness` | Verificada físicamente, con limitación | B1: el brillo cambia, pero la animación se reinicia. No hay alternativa documentada (ver «Brillo durante un efecto») |
| Cambio de efecto a color o blanco | Docs de `tapo`: *"Pre-existing lighting effect will be removed"* | Verificada físicamente | Cada pestaña reenvía su último valor |
| Lectura del efecto activo | `DeviceInfoRgbicLightStripResult`: solo `default_states.state.lighting_effect` (estado tras un corte de luz) | No soportada | La app solo sabe lo que la tira aceptó en la sesión |
| Lectura de capacidades (X6) | `RgbicLightStripHandler::get_device_info_json`, filtrada por `capabilities.rs` | Verificada físicamente como consulta | PASS de ejecución. El contenido no está revisado: no se anota ningún valor |
| Varios colores estáticos a la vez (X1) | `LightingEffect` `Static` del ejemplo oficial | Verificada físicamente (1 tira) | PASS: rojo, blanco y azul a la vez (178 ms). Rainbow después: PASS. No indica cuántos segmentos hay ni direccionamiento por LED |
| Secuencia personalizada (X2) | `LightingEffect` `Sequence` del ejemplo oficial | Verificada físicamente (1 tira) | PASS: los colores se desplazan (139 ms). Rainbow después: PASS. `direction` y `duration` siguen sin semántica |
| Segmento personalizado Circulating (X3) | `SegmentEffect` `Circulating` del ejemplo oficial | Éxito funcional observado (1 tira) | Morado que se expande de inicio a fin (190 ms). Rainbow y la otra tira: PASS. Sin confirmación IPC aparte. `[49]` no es un recuento |
| `transition` de Rainbow (X4) | `LightingEffect::with_transition` sobre el preset | Verificada físicamente (1 tira, dos valores) | 3000 más lento y 750 más rápido que 1500. Rainbow restaurado las dos veces. La otra tira no cambió. Sin unidades ni extensión a otros efectos |
| `direction` de Rainbow (X5) | `LightingEffect::with_direction` sobre el preset | **Inconcluso** | 3 y 4 mostraron movimiento, en el mismo sentido entre sí. No se comparó con `direction` 1. Aceptación IPC y *Restore Rainbow* de cada uno sin confirmar. No se afirma que inviertan el sentido. Sin selector de dirección |
| Velocidad de Rainbow (Fast / Normal / Slow) | `custom_effects::rainbow`: el preset con `transition` 750, 1500 o 3000 | Verificada físicamente (QA A 5 y 6, y QA B3 en el instalador) | Solo Rainbow y solo esos tres valores (los de X4). En todas las compilaciones. Se conserva al cambiar el brillo o volver a Effects |
| Static personalizado, 3 colores | `custom_effects::build_custom` con la estructura de X1 | Verificada físicamente (QA A 1 y 2; QA B1 en el instalador release) | Colores validados en Rust y en React. Habilitado en todas las compilaciones |
| Sequence personalizado, 3 colores | `custom_effects::build_custom` con la estructura de X2 | Verificada físicamente (QA A 3 y 4; QA B2 en el instalador release) | Igual que Static, con `direction` 1 y `duration` 50 fijos. Sin control de velocidad |
| Otros efectos personalizados | `LightingEffect::new` y `SegmentEffect::new` | Documentada por la dependencia (ejemplo) | X3 sigue solo como experimento; no está en el editor |
| Parámetros de los presets (velocidad, dirección, transiciones, distribución) | Campos de `LightingEffect` | Experimental | Sin unidades ni semántica documentadas. Ver «T2.3» |
| Control por LED individual | — | No soportada | Ninguna API de `tapo` 0.8.10 lo documenta. RGBIC no equivale a control por LED |
| Animaciones generadas en la app (envío continuo) | — | No soportada (por decisión) | Prohibido hasta medir los límites del dispositivo |

## Efectos clásicos

`id` es el identificador del contrato IPC y de los presets guardados; no debe renombrarse. Los valores internos de 0.8.10 coinciden con los de 0.8.8.

| `id` | Nombre visible | Tipo en `tapo` |
| --- | --- | --- |
| `aurora` | Aurora | Sequence |
| `bubbling_cauldron` | Bubbling Cauldron | Random |
| `candycane` | Candy Cane | Sequence |
| `christmas` | Christmas | Random |
| `flicker` | Flicker | Random |
| `grandmas_christmas_lights` | Grandma's Christmas Lights | Sequence |
| `hanukkah` | Hanukkah | Random |
| `haunted_mansion` | Haunted Mansion | Random |
| `icicle` | Icicle | Sequence |
| `lightning` | Lightning | Random |
| `ocean` | Ocean | Sequence |
| `rainbow` | Rainbow | Sequence |
| `raindrop` | Raindrop | Random |
| `spring` | Spring | Random |
| `sunrise` | Sunrise | Pulse |
| `sunset` | Sunset | Pulse |
| `valentines` | Valentines | Random |

## Efectos segmentados

- `set_segment_effect(impl Into<SegmentEffect>)` ejecuta `validate()`, envía `apply_segment_effect_rule` y enciende la tira.
- **55** presets: Birthday, Blue, Bonfire, Candlelight, Carnival, Cyan, Dancing, Dating, Disco, Dreamland, ElectroDance, Energetic, Excited, Fall, Family, Fireworks, FlowerField, Forest, Game, Green, Halloween, Happy, Jazz, Lake, LightGreen, Lyric, Moonlight, Morning, Movie, NewYear, Night, Orange, Pink, Purple, Quiet, Red, Relaxed, Rock, Siren, Sleep, Snow, Star, Study, Summer, Sunny, Sweet, Tense, Thinking, Universe, Volcano, Warm, White, Winter, Work, Yellow.
- Cada preset envía solo `brightness`, `custom` (0), `display_colors`, `enable` (1), `id` (`TapoStrip_…`) y `name`. No define `type`, `segments` ni `states`: el patrón lo decide el firmware a partir del `id`. Lo comprueba el test `segment_presets_only_reference_firmware_patterns`.
- Brillo por defecto: 50, salvo Moonlight (25).
- En la app: catálogo `device::SEGMENT_EFFECT_CATALOG`; el `id` es el `name` snake_case de `tapo` (p. ej. `electro_dance`), y el nombre visible lleva espacios (Electro Dance).

## T2.3 — Parámetros de personalización

### Evidencia disponible

- **Campos de `LightingEffect`:** no tienen comentarios de documentación (`#[allow(missing_docs)]`). Solo hay tipos Rust y los valores que usan los 17 presets.
- **Campos de `SegmentEffect`:** tienen una línea de documentación cada uno. Solo `brightness` indica un rango («between 0 and 100»).
- **Funciones del handler:** solo se documentan rangos para `set_brightness` (1–100), `set_hue_saturation` (0–360 y 1–100) y `set_color_temperature` (2500–6500).
- **Ejemplo oficial `examples/tapo_l930.rs`:** construye tres efectos personalizados:
  - `LightingEffectType::Static` con `segments [0, 1, 2]`, tres colores en `sequence` y `expansion_strategy 1`;
  - el mismo efecto como `Sequence` con `direction 1` y `duration 50`;
  - un `SegmentEffect` `Circulating` con `segments [49]` y un estado `[267, 56, 100, 0]`.

  El ejemplo no dice qué hace cada valor ni en qué modelo se probó.
- **Validación de `tapo`:** no valida rangos. `SegmentEffect::validate()` solo exige `segments`, `states` y `type` cuando el efecto es personalizado.

Un tipo `u8` o `u16` no define el rango válido. Los rangos de abajo son **valores observados** en presets o en el ejemplo, no rangos aceptados por el firmware.

### Matriz de parámetros

| # | Parámetro | Campo y tipo | Unidades | Valores observados | Rango documentado | Semántica | Clasificación |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | Velocidad | No hay un campo llamado velocidad. En Rainbow, `transition: u16` mediante `with_transition` cambia la velocidad observable | No documentadas | 1500 es el preset. 3000 fue más lento y 750 más rápido en una L930-5. Sunrise usa 60000 | Ninguno | En Rainbow, y solo en esos dos valores, un número mayor fue más lento. No se generaliza | Verificado en Rainbow (X4). Control Fast/Normal/Slow implementado solo para Rainbow. `SegmentEffect`: no soportada |
| 2 | Varios colores a la vez | `LightingEffect.sequence: Vec<[u16; 3]>` con `segments` y `expansion_strategy`; `SegmentEffect.states: Vec<[u16; 4]>` | `[tono, saturación, valor]`, deducido de los presets | Tono 0–360, saturación 0–100, valor 0–100 en presets; `states` y `display_colors` siempre con `[h, s, 100, 0]` en los 55 presets | Ninguno para estos campos | X1 mostró rojo, blanco y azul a la vez en distintas partes. No se sabe cuántos segmentos hay ni cómo se reparte `segments [0, 1, 2]` | Verificado en un `Static` fijo (X1). El reparto y los segmentados personalizados siguen sin confirmar |
| 3 | Orden de colores | Orden de `sequence` | — | Rainbow: 0, 100, 200, 300; Candy Cane: 16 entradas | Ninguno | Hipótesis: se recorre en orden en efectos `Sequence` | Experimental |
| 4 | Intensidad y brillo | `brightness: u8` en ambos tipos; `brightness_range: Vec<u8>` en `LightingEffect` | Porcentaje | Clásicos 100; segmentados 50 y 25; `brightness_range` [0, 80], [10, 30], [50, 100], [90, 100] | `SegmentEffect.brightness` 0–100; `set_brightness` 1–100 | `brightness`: intensidad del efecto. Cómo combina con `brightness_range` no está documentado | `brightness`: **implementado** (la app valida 1–100). `brightness_range`: experimental |
| 5 | Dirección | `direction: Option<u8>`, mediante `with_direction` | — | Rainbow usa 1. X5 probó 3 (Ocean) y 4 (Aurora, Icicle) sin cambiar `transition` | Ninguno | Desconocida. 3 y 4 se movieron en el mismo sentido; no se comparó con 1 | X5 inconcluso. Sin control en la UI. `SegmentEffect`: no soportada |
| 6 | Duración de transiciones | `transition`, `fade_off: u16`, `run_time: u64`, `repeat_times: u8`, `duration` | No documentadas | `fade_off` 150–3000; `repeat_times` 0 (Sequence) y 1 (Sunrise, Sunset); `run_time` 0 | Ninguno | Desconocida | Experimental |
| 7 | Distribución de segmentos | `segments: Vec<u8>`, `segment_length: u8`, `expansion_strategy: u8`, `spread: u8`; en `SegmentEffect`, `segments` | — | `segments` [0], [1], [80], 0–15 (Candy Cane), índices hasta 80 (Lightning), [49] (ejemplo segmentado); `expansion_strategy` 1 y 2; `spread` 1, 3, 7, 12, 16; `segment_length` sin uso | Doc de `SegmentEffect.segments`: «typically the total segment count» | No se sabe cuántos segmentos tiene el L930-5 ni qué indican los índices | Experimental |
| 8 | Efectos personalizados | `LightingEffect::new(…, is_custom, …)` con `Sequence`, `Random`, `Pulse`, `Static`; `SegmentEffect::new` con `Circulating`, `Breathe`, `Chasing`, `Flicker`, `Bloom`, `Stacking`, `None` | — | Los tres efectos del ejemplo, más Rainbow con otro `transition` o `direction` | Ninguno | `Static` (X1) muestra tres colores. `Sequence` (X2) los desplaza. `Circulating` (X3) se vio como un morado que se expande. `transition` (X4) cambia la velocidad de Rainbow | X1, X2 y X4: verificados. X3: éxito visual, confirmación IPC independiente pendiente. X5: inconcluso. Editor Static/Sequence: en todas las compilaciones, verificado también en el instalador (QA B1 y B2). El resto de personalizaciones sigue experimental |

### Evaluación

- **Se puede cambiar ya, con semántica documentada:** solo `brightness`, en ambos tipos. Ya está implementado.
- **Exige construir un efecto personalizado:**
  - en efectos segmentados, todo salvo el brillo, porque los presets no envían `type`, `segments` ni `states`;
  - en efectos clásicos, varios colores, orden, dirección, transiciones y distribución. Se pueden poner sobre un preset, pero no hay rangos ni unidades.
- **Requiere validación física:** todo lo anterior. El brillo ya está validado: en los segmentados, reenviarlo reinicia la animación (B1).
- **No soportado:**
  - velocidad o dirección en efectos segmentados (no existen esos campos);
  - control por LED;
  - leer el efecto activo.
- **Varios colores a la vez:** X1 (`Static`, `segments [0, 1, 2]`, rojo, blanco y azul) fue aceptado en una L930-5. El usuario vio los tres colores a la vez, en distintas partes de la tira. Eso no indica cuántos segmentos físicos tiene ni permite un color por LED. `segments [0, 1, 2]` sigue sin semántica documentada.

### Decisión (consolidación T2)

Solo se exponen controles respaldados por una prueba física:

- **Velocidad de Rainbow:** Fast (750), Normal (1500), Slow (3000). Valores cerrados en Rust (`RainbowSpeed`) y en React. Rechazada para cualquier otro efecto.
- **Static y Sequence personalizados (todas las compilaciones desde la estabilización):** tres colores sobre las estructuras de X1 y X2. Todo lo demás del payload es fijo: `custom` 1, `enable` 1, `expansion_strategy` 1, `segments [0, 1, 2]`; en Sequence, `direction` 1 y `duration` 50. `sequence` lleva los tres colores y `display_colors` el primero, como en el ejemplo de `tapo`. Los tres índices de `segments` no se interpretan como tres zonas físicas.
- **Límites de color** (`validation::hue`, `effect_saturation`, `effect_value`): tono 0–360, saturación 0–100, valor 1–100. `tapo` no valida los colores de un efecto; son los rangos de sus presets, salvo valor 0 (apagado), que no se acepta.
- Sin controles de dirección, transición de otros efectos, distribución ni segmentos.

Plan de pruebas controladas: `t2-report.md`, sección 7. QA: `t2-qa-checklist.md`. Parte A 10/10 PASS. Parte B 7/7 PASS, incluida la prueba 4 (X1–X5 ocultos en el instalador release, confirmación visual del usuario).

- X6 (lectura): la consulta PASS. Contenido sin revisar.
- X1 (`Static`): PASS. Rainbow después: PASS.
- X2 (`Sequence`): PASS, con desplazamiento visible. Rainbow después: PASS.
- X3 (`Circulating`): éxito visual. Confirmación IPC independiente pendiente. `[49]` no es un recuento de segmentos.
- X4 (`transition` 3000 y 750): PASS en una L930-5. 3000 más lento, 750 más rápido. Sin unidades ni extensión a otros efectos.
- X5 (`direction` 3 y 4): **inconcluso**. Hubo movimiento, en el mismo sentido con 3 y con 4. No se comparó con 1, ni se confirmaron la aceptación IPC ni *Restore Rainbow* de cada uno. La segunda tira no cambió.
- QA parte A (consolidación): **10/10 PASS**, sin observaciones. Static y Sequence con dos paletas, Rainbow Fast/Normal/Slow, brillo con Slow, cambios entre modos, independencia de las tiras, *Restore Rainbow* con reconexión, e instalador anterior con los controles experimentales ocultos.
- QA parte B (estabilización): **7/7 PASS**, sin observaciones. Custom Static, Custom Sequence y Rainbow Speed en el instalador release; X1–X5 ocultos en ese instalador (confirmación visual del usuario); reconexión independiente; errores de Color, White y colores rápidos; cambios rápidos.

## Brillo durante un efecto

- `tapo` documenta que `set_brightness`, `set_hue_saturation` y `set_color_temperature` **eliminan** el efecto activo.
- En los modos de efecto, con un efecto enviado en la sesión, el control de brillo reenvía el último efecto de esa tira con otro `brightness` (debounce de 400 ms por tira), incluida su velocidad de Rainbow o sus colores personalizados. Fuera de ese caso se usa `set_brightness`.
- Elegir un efecto o cambiar de pestaña envía el efecto con el brillo actual del control deslizante.
- **Limitación conocida (B1):** reenviar un efecto segmentado con otro brillo reinicia su animación. Aceptada provisionalmente.
- Alternativas revisadas en `tapo` 0.8.10, ninguna válida:
  - `set_brightness` y `set().brightness()` envían `set_device_info`, que según la documentación elimina el efecto;
  - no existe una petición para cambiar solo el brillo de un efecto activo.
  
  No se usan comandos repetitivos ni peticiones no documentadas.
- Pendiente de validar: cómo combina el firmware `brightness` con `brightness_range` (p. ej. Lightning `[90, 100]`).

## Lectura de capacidades (X6)

- `get_device_info_json` es público en `tapo` 0.8.10, sin features condicionales. `tapo` solo tiene las features `default = []` y `python`.
- Mismo request que `get_device_info`, que la app ya usa al conectar: solo lectura.
- `capabilities::filter`:
  - devuelve valores solo de `model`, `type`, `hw_ver`, `fw_ver`, `device_on`, `brightness`, `color_temp`, `color_temp_range`, `hue`, `saturation` y `overheated`;
  - de `lighting_effect`, si existe, solo `enable`, `custom`, `name`, `type`, `brightness` y `segments`.
  
  Del resto solo devuelve el nombre de la clave. Los campos permitidos que traigan un objeto inesperado se descartan.
- Comando `device_lighting_capabilities(ip)`: IP obligatoria, sin log del contenido.
- Resultado en las tiras: la consulta **funcionó**. El contenido filtrado todavía no se ha recibido. No se anota ningún campo, valor ni número físico de segmentos, y no se vuelve a consultar el dispositivo para obtenerlo.

## Detección RGBIC

`ApiClient::l930()` no verifica el modelo. La app guarda el `model` que reporta `get_device_info` al conectar y solo envía efectos si el handle es RGBIC **y** el modelo empieza por `L920`/`L930` (`device::is_rgbic_model`). En otro caso el backend rechaza el comando, la UI deshabilita el panel de efectos y el resto de controles sigue funcionando.

## Concurrencia

- `CommandGate` (un `tokio::sync::Mutex` por dispositivo) serializa los comandos de escritura de cada tira y los atiende en orden de llegada (test `command_gate_runs_commands_in_arrival_order`). Cada tira tiene su propia puerta.
- La latencia mostrada incluye la espera en la cola.
- La puerta es por IP y estable durante todo el proceso (`gate_for`). Al reconectar se crea un `TapoDevice` nuevo, pero recibe la misma puerta, así que un comando aún en curso con la conexión sustituida termina antes de que empiece el siguiente. Corregido en la estabilización de T2; antes cada conexión tenía su propia puerta.
- La puerta solo cubre escrituras. El login de una reconexión y las lecturas (`get_device_info`, X6) no esperan a la puerta.

## Restricciones por compilación

| Función | `npm run dev` | Instalador release | Control en backend |
| --- | --- | --- | --- |
| 17 clásicos, 55 segmentados, velocidad de Rainbow | Sí | Sí | Validación de nombre, brillo y velocidad |
| Custom Static / Sequence | Sí | Sí | `device_custom_effect`: IP explícita, petición cerrada, colores validados |
| *Read lighting capabilities* (X6) | Sí | Sí | Solo lectura, filtrada por `capabilities.rs`, IP explícita |
| Experimentos X1–X5 y su *Restore Rainbow* | Sí | No | `device_experiment` comprueba `experiments_enabled()` (`debug_assertions`) antes de buscar el dispositivo |

Las dos restricciones son independientes: Custom no depende de `experiments_enabled()`.

## Actualización a 0.8.10 (aplicada, aprobada por el usuario)

Cambio en `src-tauri/Cargo.toml`:

```toml
tapo = "0.8.10"
# tapo >= 0.8.9 needs `rsa::Pkcs1v15Encrypt` but does not enable the `pkcs1v15`
# feature it requires since rsa 0.10.0.
rsa = { version = "0.10", default-features = false, features = ["pkcs1v15"] }
```

- `tapo = "0.8.10"` fija el mínimo con `set_segment_effect`. El lockfile está en 0.8.10 exacta; un `cargo update` futuro podría subir a 0.8.12.
- La línea `rsa` es necesaria y suficiente: sin ella falla exactamente un import (`rsa::Pkcs1v15Encrypt`), y con ella compila. `default-features = false` evita activar otras características de `rsa`. El proyecto no usa `rsa` directamente. Se puede retirar cuando `tapo` active `pkcs1v15`.
- El lockfile se generó con `cargo update -p tapo --precise 0.8.10` desde el de `develop`: 20 paquetes salen y 31 entran, todos del árbol criptográfico y HTTP de `tapo`. `tokio`, `serde`, `serde_json`, `tauri`, `keyring` y `chrono` no cambian. `uuid` sube de 1.20 a 1.29 porque `tapo` 0.8.10 lo exige.
- Código de `tapo` que usa la app, comparado entre 0.8.8 y 0.8.10:
  - KLAP y *passthrough*: misma lógica; cambia la fuente de aleatoriedad (`OsRng` → `StdRng` sembrado desde `SysRng`).
  - `ApiClient`, `RgbicLightStripHandler`, `LightingEffect`, `Color` y `DeviceInfoRgbicLightStripResult`: sin cambios de comportamiento.
  - Renombres `S200B*` → `S200*`: no los usa la app.
- Estado: **aplicada**. Regresión física R1–R9 PASS en las dos tiras, según el usuario.

### Evaluación previa

- Manifiesto original: `tapo = "0.8"`. 0.8.9 y 0.8.10 arrastran las mismas dependencias transitivas.
- Cambios transitivos principales: `rsa` 0.9.10 → 0.10.0, `der` 0.7 → 0.8, `pkcs1` 0.7 → 0.8, `pkcs8` 0.10 → 0.11, `spki` 0.7 → 0.8, `signature` 2 → 3, `const-oid` 0.9 → 0.10, `uuid` 1.20 → 1.29. Entran `reqwest` 0.13.5 (convive con 0.12.28), `sha2` 0.11, `digest` 0.11, `hmac` 0.13 y `rand` 0.10.
- **Incompatibilidad:** `tapo` 0.8.9 a 0.8.12 declaran `rsa = "0.10.0-rc.17"`. Cargo resuelve `rsa` 0.10.0, donde `Pkcs1v15Encrypt` necesita la característica `pkcs1v15`, que `tapo` no activa: `error[E0432]: unresolved import rsa::Pkcs1v15Encrypt`. Es un defecto de empaquetado de `tapo`.
- Alternativas probadas:
  - Fijar `rsa` 0.10.0-rc.17: **falla** con 9 errores dentro de `rsa`.
  - Añadir la línea `rsa` con `pkcs1v15`: **compila** sin cambiar código del proyecto.
- Riesgo: `rsa` cifra el handshake *passthrough*, respaldo del login KLAP. Quedó cubierto por la regresión física (login, reconexión, Remember Me).
