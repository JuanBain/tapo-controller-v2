# T1 — Auditoría, preparación y seguridad: reporte

Rama: `feature/t1-baseline-security` (desde `develop` = `origin/master` = `upstream/master` `46b5ee1`). Sin commits, push ni PR.

Detalle técnico en `architecture.md`, `security.md` y `development.md`. Este documento resume el trabajo sin repetirlos.

## 1. Auditoría inicial

- **Git**: `origin` apunta a `JuanBain/tapo-controller-v2` y `upstream` a `BartaG512/tapo-controller`. Existen `master` y `develop` (local y en origin). No hay worktrees adicionales.
- **Working tree previo**: `Cargo.toml` y `gen/schemas/*.json` aparecían modificados solo por fin de línea (LF/CRLF, efecto de la compilación); su contenido es idéntico. `.cursor/` está sin versionar. No se tocó nada de eso.
- **Pruebas existentes**: ninguna (`cargo test` ejecutaba 0). El único workflow (`build.yml`) compila y publica releases en tags `v*`; no corre en PRs ni ejecuta pruebas.
- **Diferencia con las reglas**: `60-testing-ci-docs.mdc` menciona `docs/architecture-audit.md`. Se siguió la instrucción explícita de T1 (`docs/architecture.md` + `t1-report.md`).

## 2. Problemas identificados

| ID | Área | Problema | Acción |
|---|---|---|---|
| A1 | Credenciales | Contraseña en texto plano en `data/credentials.json`, que era la fuente real de la sesión y la reconexión | Corregido + migración |
| A2 | Credenciales | La contraseña del keyring se enviaba al webview | Corregido |
| A3 | Credenciales | Remember me no tenía efecto real | Corregido |
| A4 | Tauri | Permisos `store:default` y `shell:allow-open` sin uso; `csp: null` | Corregido |
| A5 | IPC | Conversiones `as u8` / `as u16` truncaban (p. ej. brillo 300 → 44; −10 → 65526) | Validación con `try_from` y rangos de `tapo` |
| A6 | IPC | Saturación 0 aceptada en IPC y rechazada después por `tapo` | Rango 1–100 en IPC |
| A7 | Multi-dispositivo | `HashMap.values().next()` en 6 comandos, la bandeja y el scheduler: dispositivo arbitrario con 2 tiras | `resolve_target` |
| A8 | Concurrencia | `RwLock` global retenido durante la E/S de red (`list_devices`, controles) | `Arc<TapoDevice>` + liberar el lock |
| A9 | Conexión | `l930 → l920 → l900` hacen el mismo login: hasta 3 intentos × 30 s | Un intento, timeout 10 s |
| A10 | Arranque | La sesión dependía de leer el archivo; el scheduler esperaba a que terminara la reconexión | Restauración síncrona; scheduler primero |
| A11 | Scheduler | Un horario inválido abortaba la carga de todos | Se registra y se omite |
| A12 | Scheduler | `time` se insertaba sin validar en la expresión cron | Validación `HH:MM` y días 0–6 |
| A13 | Persistencia | `fs::write` no atómico; un JSON corrupto se sobrescribía en el siguiente guardado (pérdida silenciosa) | Escritura atómica + respaldo `.corrupt-<fecha>` |
| A14 | Persistencia | Lectura-modificación-escritura sin exclusión (guardados concurrentes perdidos) | `Mutex` en `Storage` |
| A15 | Duplicación | Aplicar un preset estaba duplicado en `commands.rs` y `scheduler.rs` | `TapoDevice::apply_preset` |
| A16 | Frontend | Añadir un dispositivo tras reiniciar sin Remember me enviaba contraseña vacía | `add_device` usa la sesión |
| A17 | Frontend | 3 errores de tipos (`useRef()` sin argumento, React 19), ocultos porque `vite build` no comprueba tipos | Corregido |

Observaciones **no corregidas** (fuera del alcance de T1 o sin impacto funcional):
- Las llamadas `invoke` de control en `App.tsx` no tienen `try/catch`; los errores no se muestran en la UI.
- `list_devices` puede ejecutarse antes de que termine `auto_reconnect`, y la UI muestra entonces las tiras como desconectadas hasta refrescar.
- `discovery.rs` solo detecta el puerto 80 abierto, sin identificar dispositivos Tapo.
- Sin el model check, una L900 se trataría como RGBIC (sin cambios respecto a upstream).
- UI de efectos RGBIC inexistente (el backend sí la tiene): queda para T2.

## 3. Cambios implementados

| Archivo | Motivo |
|---|---|
| `src-tauri/src/validation.rs` (nuevo) | A5, A6, A12; validación de IP y email |
| `src-tauri/src/credentials.rs` | A1–A3: `SecretStore`, `CredentialManager`, sesión, migración verificada |
| `src-tauri/src/storage.rs` | A13, A14, `account.json`, lectura de solo lectura de los formatos heredados; `Schedule.device_ip` opcional |
| `src-tauri/src/device.rs` | A7–A9, A15, redacción de errores |
| `src-tauri/src/commands.rs` | Frontera IPC con validación, `resolve_target` y comando `add_device`; mismos nombres de comandos |
| `src-tauri/src/scheduler.rs` | A7, A11, A12, A15 |
| `src-tauri/src/lib.rs` | A10, bandeja con destino explícito, registro de `add_device` |
| `src-tauri/tauri.conf.json` | CSP |
| `src-tauri/capabilities/default.json` | Permisos mínimos |
| `src-tauri/gen/schemas/capabilities.json` | Regenerado por Tauri a partir de las capabilities |
| `src/App.tsx` | `remember_me` en el login, `add_device`, borrar la contraseña del estado, A17 |
| `docs/*.md` | Documentación T1 |

**Dependencias**: ninguna añadida ni actualizada. `Cargo.lock` y `package-lock.json` sin cambios. La CLI de Tauri descargó a su caché las herramientas de empaquetado (WiX 3.14, NSIS 3.11).

**Cambios de comportamiento a tener en cuenta**:
- Con 2 tiras conectadas, las acciones rápidas de la bandeja y los horarios sin `device_ip` **no actúan** y lo registran en el log, en lugar de elegir una tira al azar. Definir el destino de la bandeja corresponde a T3.
- Los valores fuera de rango ahora devuelven un error claro en lugar de truncarse.
- Sin Remember me hay que iniciar sesión en cada arranque.
- Si la conexión con un dispositivo falla, las credenciales ya no se guardan.

## 4. Pruebas ejecutadas (sin hardware)

| Comando | Resultado |
|---|---|
| `npm ci` | PASS (142 paquetes) |
| `npx tsc --noEmit -p .` | Base: FAIL (3 errores previos). Final: **PASS** |
| `npm run build:frontend` | **PASS** |
| `cargo check` | **PASS**, 0 warnings |
| `cargo clippy` | **PASS**, 0 warnings |
| `cargo test` | Base: 0 pruebas. Final: **PASS 49/49** |
| `npm run build` | **PASS**: exe release + MSI + NSIS |
| `npm audit --omit=dev` | 0 vulnerabilidades en producción |

Cobertura de las 49 pruebas:
- Validación de parámetros (8).
- Persistencia (8): roundtrip, JSON corrupto, escritura atómica, concurrencia, compatibilidad de `schedules.json`, archivos heredados intactos.
- Credenciales (19): Remember me sí/no, restauración, cierre de sesión, todas las ramas de la migración, ausencia del secreto en JSON, `Debug`, respuesta IPC y borrado del texto plano solo tras reinicio verificado y reconexión.
- Selección de dispositivo (5).
- Errores y redacción (4).
- Scheduler (2) y peticiones IPC (3).

Reejecución final (16 nov 2026, 12:1x): `tsc` PASS, `build:frontend` PASS, `cargo check` PASS (0 warnings), `cargo clippy` PASS (0 warnings), `cargo test` **49/49 PASS**. `npm run build` no se repitió: no hubo cambios de código desde la última ejecución.

Las pruebas automatizadas no usan hardware ni el Credential Manager real. Las pruebas físicas las hizo el usuario (§6).

## 5. Riesgos pendientes

1. **La app original de upstream sigue instalada** (`C:\Program Files\Tapo Controller`) y se inicia con Windows (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` → `Tapo Controller`). Comparte el directorio de datos. Si inicias sesión en ella, vuelve a escribir la contraseña en texto plano en `data\credentials.json`. **Acción manual recomendada**: quitarla del inicio automático (Configuración → Aplicaciones → Inicio) o desinstalarla antes de usar el build del fork.
2. La CSP del build release/instalado no se ha validado visualmente. Las pruebas físicas se hicieron con `npm run dev`, donde la CSP no se aplica.
3. La bandeja con 2 tiras no hace nada (ver §3).
4. 10 vulnerabilidades npm en herramientas de build (detalle en `security.md`).
5. Sin CI en PRs. Propuesta: workflow de Windows con `tsc`, `build:frontend` y `cargo test` (T5 o con tu aprobación).
6. La contraseña en memoria no se borra de forma segura (`zeroize` no añadido).

### Para T2 — RGBIC

- No hay UI de efectos ni control por segmentos. El backend sigue exponiendo los 17 presets de `tapo` 0.8.8 sin prueba física. T2 no debe asumir que funcionan en las L930-5.
- El 16 nov 2026, entre 12:07 y 12:08, el dispositivo rechazó credenciales 7 veces (`Local hash does not match server hash`). Un rechazo repetido puede bloquear temporalmente la tira. T2 debe mostrar ese error y no reintentar en ráfaga.

### Para T7 — distribución y convivencia

- **CSP del release.** La política solo se aplica a los assets empaquetados. Las pruebas físicas se hicieron con `npm run dev`, donde no actúa. Falta abrir el `.exe` de release o el instalador y confirmar que el selector de color y los estilos se ven bien. `style-src 'unsafe-inline'` sigue siendo necesario por `react-colorful`.
- **Bandeja con dos tiras.** Con más de un dispositivo conectado, Encender, Apagar y los brillos de la bandeja no actúan. Antes elegían una tira al azar. Definir el destino (una tira, la selección o ambas) queda fuera de T1.
- **Convivencia con la app original.** `C:\Program Files\Tapo Controller` sigue instalada y en el inicio de Windows. Comparte `%APPDATA%\com.bartag.tapo-controller`. Un login en esa versión vuelve a crear `data\credentials.json` con la contraseña en texto plano. Antes de distribuir el fork hay que desactivar ese inicio automático o desinstalar la app original.

## 6. Checklist de pruebas físicas

> **Estado**: el usuario informa (16 nov 2026) que realizó las pruebas físicas con las dos L930-5 usando `npm run dev`. Los resultados por ítem **no se han registrado aquí todavía**; la columna se deja para que el usuario la marque. El agente no ejecutó ninguna prueba sobre las luces. Ver §8 para lo verificado por el agente sobre el estado resultante.

Preparación: cierra cualquier instancia de la app instalada, incluida la de la bandeja. Para ver los logs, ejecuta `RUST_LOG=info npm run dev` en Git Bash; el `.exe` release no tiene consola.

| # | Prueba | Esperado | Resultado |
|---|---|---|---|
| 1 | Primer arranque | Log `Credential restore: migration=PasswordMigrated` (o `NotRemembered` / `PreferencesOnly`). Entras sin login si tenías Remember me | ☐ |
| 2 | Credential Manager → Credenciales de Windows | Existe una credencial genérica cuyo nombre contiene `tapo-password` y `tapo-controller` | ☐ |
| 3 | `data\account.json` | Solo email y `remember_me`, sin contraseña | ☐ |
| 4 | UI completa | Selector de color con estilos correctos; sin pantalla en blanco (CSP) | ☐ |
| 5 | Tira 1: encender y apagar | La tira responde; el indicador de la UI se actualiza | ☐ |
| 6 | Tira 1: brillo 1 %, 50 % y 100 % | Cambia sin error | ☐ |
| 7 | Tira 1: color (rojo, verde, azul de los colores rápidos) y arrastrar el selector | Cambia el color; sin errores con saturación ≥ 1 | ☐ |
| 8 | Tira 1: modo White, 2500 K y 6500 K | Cambia la temperatura | ☐ |
| 9 | Añadir tira 2 (`+` → IP) | Se conecta sin pedir contraseña; aparecen 2 tarjetas | ☐ |
| 10 | Seleccionar tira 2 y cambiar el color | **Solo** cambia la tira 2 | ☐ |
| 11 | Seleccionar tira 1 y cambiar el brillo | **Solo** cambia la tira 1 | ☐ |
| 12 | Reconexión: desenchufa la tira 2 ~30 s, reconéctala y pulsa ⟳ | Error controlado mientras está apagada; reconecta después | ☐ |
| 13 | Reinicio con Remember me | Las 2 tiras se reconectan solas (unos segundos) | ☐ |
| 14 | Bandeja con 2 tiras: "Turn On" | No actúa; log `Tray action skipped` | ☐ |
| 15 | Bandeja con 1 tira | Actúa sobre esa tira | ☐ |
| 16 | Cerrar sesión | La credencial desaparece del Credential Manager y `data\credentials.json` desaparece | ☐ |
| 17 | Login sin Remember me y reinicio | Pide login; no hay entrada `tapo-password` | ☐ |
| 18 | Contraseña errónea al añadir una tira | Mensaje de error sin la contraseña ni el email | ☐ |

## 7. Estado final

**T1: COMPLETADA, con observaciones.**
- Código, documentación y validación automatizada están completos y en PASS.
- El usuario informa que hizo las pruebas físicas.
- El estado resultante de las credenciales cumple el objetivo de seguridad (§8).

Antes de integrar en `develop` conviene que el usuario marque los ítems del §6 y aplique la acción manual del §5.1.

No se ha avanzado a T2. Sin commits, push ni merges.

## 8. Verificación final del estado de credenciales (agente, 16 nov 2026 12:1x)

Comprobaciones sin mostrar ni registrar valores; solo resultados booleanos y tamaños.

| Comprobación | Resultado |
|---|---|
| `data\credentials.json` (texto plano heredado) | **No existe.** Ya no hay contraseña heredada en disco |
| `data\account.json` | Claves `email` y `remember_me`; `remember_me = true`; sin contraseña |
| Windows Credential Manager, `tapo-password.tapo-controller` | Presente, no vacía; dos lecturas consecutivas idénticas; persistencia `CRED_PERSIST_ENTERPRISE` (tipo 3, el valor por defecto de `keyring`; acompaña al perfil itinerante si existe) |
| Datos de entrada para restaurar la sesión | Correctos: email, remember_me = true y secreto recuperable. Restauración de sesión **verificada por código** (pruebas unitarias) y **reportada físicamente** por el usuario (la app arrancó a las 12:10:35 tras el login de las 12:10:03) |
| Búsqueda de la contraseña real en los JSON de datos | 0 claves `password` en los 4 archivos |
| Búsqueda de la contraseña real en el repositorio (54 archivos) y en los logs de terminal | 0 coincidencias |
| `devices.json` | 2 dispositivos (`192.168.1.151`, `192.168.1.217`), 254 bytes; igual que antes |
| `settings.json` | `last_device_ip`, 39 bytes; igual que antes |
| Store del plugin (`credentials.json` raíz) | `email`, `rememberMe`; sin contraseña |

**Cómo desapareció el archivo heredado.** Los logs disponibles no lo determinan, porque las sesiones de `npm run dev` del usuario no tenían `RUST_LOG=info`. Hay dos caminos y ambos son aceptables según lo acordado:
- **Borrado verificado automático** en el arranque de las 12:10:35: migración ya hecha, contraseña releída del Credential Manager e idéntica a la del archivo, y las dos tiras reconectadas.
- **Cierre de sesión explícito** desde la app, que borra el archivo por diseño.

En ningún caso se eliminaron otros archivos ni dispositivos guardados.

**Punto 4 (Remember me desactivado)**: no aplica en el estado final, porque Remember me está activado. La versión actual no borra el archivo heredado tras un login manual sin Remember me: lo conserva, porque no puede verificarse sin persistir la contraseña. Añadir ese caso sería un cambio de comportamiento nuevo y no se ha hecho.

**Observación de los logs del usuario**: entre 12:07 y 12:08 hubo 7 errores `klap_protocol: Local hash does not match server hash`. `tapo` los emite cuando el dispositivo rechaza las credenciales en el handshake, por ejemplo con una contraseña incorrecta o una sesión anterior inválida. Desde las 12:10 no aparecen más. Un rechazo repetido puede provocar un bloqueo temporal en el dispositivo; conviene tenerlo en cuenta en las pruebas de T2.
