# Operar Hécate

Los procedimientos para poner Hécate en marcha y para cada situación de operación: arrancar tras un reinicio, recuperar
una frase olvidada, perder el disco, renovar o sustituir la CA y responder a una clave comprometida. Cada uno dice
cuándo se aplica, qué hace falta, los pasos con las órdenes de la [CLI](cli.md) y cómo comprobar que ha ido bien. Es
para el admin.

## Antes de empezar

- Todas las órdenes se ejecutan en el equipo de Hécate, con el mismo usuario y las mismas variables de entorno que
  `hecate serve` ([CLI y configuración](cli.md#variables-de-entorno)).
- `hecate serve` corre en primer plano y se para con SIGINT o SIGTERM (Ctrl-C en su terminal): borra la raíz de la
  memoria y sale. Cómo se deja corriendo como servicio en el home lab es una decisión
  [abierta](../../DECISIONS.md#abiertas); los procedimientos solo dicen cuándo arrancarlo o pararlo.
- El lado de cada componente (darse de alta, renovar, resolver) está en la
  [guía de integración](integracion.md).

| Situación | Procedimiento |
|---|---|
| Equipo sin Hécate | [Puesta en marcha](#puesta-en-marcha) |
| Reinicio del equipo o del proceso, o corte de luz | [Arrancar y desbloquear](#arrancar-y-desbloquear-tras-un-reinicio-o-un-corte) |
| Hécate caída o sellada | [Hécate caída o sellada](#hécate-caída-o-sellada) |
| Se pierde el disco | [Copia en frío y restauración](#se-pierde-el-disco) |
| Se olvida la frase | [Desbloquear con dos partes](#se-olvida-la-frase) |
| Se pierden la frase y las partes | [Empezar de cero](#se-pierden-la-frase-y-las-partes) |
| La intermedia va a caducar | [Renovar la intermedia](#la-intermedia-va-a-caducar) |
| CA intermedia comprometida, o clave de la CA raíz o su frase perdidas | [Sustituir la CA](#ca-comprometida-o-perdida) |
| Clave de un componente comprometida | [Revocar o deshabilitar](#clave-de-un-componente-comprometida) |
| Clave propia de Hécate comprometida | [Revocar su certificado](#clave-propia-de-hécate-comprometida) |
| Clave de ámbito comprometida | [Lo que se puede hacer](#clave-de-ámbito-comprometida) |

## Puesta en marcha

**Cuándo:** la primera vez, en un equipo sin Hécate.

**Qué necesitas:**

- el binario `hecate` y un usuario propio para Hécate en su equipo;
- las variables de entorno (al menos `HECATE_DATA_DIR`) y lo que Hécate necesita del sistema, como el permiso para
  bloquear memoria en Linux ([CLI](cli.md#lo-que-necesita-del-sistema));
- tres sitios distintos para las partes de recuperación y dos soportes distintos para la copia de la clave de la CA
  raíz;
- el nombre de identidad del primer componente y sus secretos.

**Pasos:**

1. Arranca `hecate serve`. Queda sin inicializar y sin API.
2. Inicializa: `hecate init`. Responde `yes`, fija la frase dos veces (12 caracteres o más), guarda cada una de las tres
   partes en un sitio distinto y confirma con dos de ellas, como mucho 15 minutos después. Al terminar, borra el
   historial del terminal.
3. Desbloquea con la frase: `hecate unseal`.
4. Crea la CA: `hecate ca create --trust-domain <td> --out <fichero>` (p. ej. `--trust-domain cerbero.home`). Fija la
   frase de la CA, distinta de la de desbloqueo; cuando lo pida, copia el fichero a sus dos soportes y da la ruta de
   una de esas copias y la frase otra vez. Apunta la huella de la CA raíz que escribe al final.
5. Guarda los secretos del componente: `hecate secrets create secret://<identidad>/<ruta> --type <tipo>`, con el valor
   sin eco o con `--from-file <fichero>`.
6. Concede sus permisos, si los necesita: `hecate policies grant-read <identidad> <ruta>` por cada secreto de otra
   identidad que deba leer, y `hecate policies grant-status <identidad>` si consulta el estado.
7. Crea su token de alta: `hecate tokens create <identidad>`, y entrégaselo al componente antes de que caduque (15
   minutos por defecto). El componente se da de alta con él.

**Cómo comprobarlo:**

- `hecate status` dice `state: unsealed` y la línea `ca:` con el trust domain y la huella;
- el log de `hecate serve` dice que la API escucha (`the API listens`);
- `hecate identities list` muestra la identidad como `enrolled`, con la caducidad de su certificado;
- el componente resuelve sus secretos.

## Arrancar y desbloquear tras un reinicio o un corte

**Cuándo:** tras reiniciar el equipo o el proceso, o tras un corte de luz. Hécate arranca siempre sellada, y mientras
lo está no hay avisos, porque dependen de secretos que custodia.

**Qué necesitas:** la frase (o, si se ha olvidado, dos partes: [se olvida la frase](#se-olvida-la-frase)).

**Pasos:**

1. Arranca `hecate serve` con la misma configuración.
2. Comprueba que está sellada: `hecate status` dice `state: sealed`.
3. Desbloquea: `hecate unseal`.

**Cómo comprobarlo:**

- `hecate status` dice `state: unsealed`;
- los componentes vuelven a resolver al reintentar;
- si Hécate estuvo sellada más tiempo que la vida de su certificado (3 días por defecto), la API había dejado de
  escuchar: Hécate lo renueva sola al desbloquearla, y el log vuelve a decir `the API listens`. Si no lo dice y
  `hecate status` muestra `intermediate days left: 0`, la intermedia también caducó y Hécate no puede renovarlo: sigue
  [la intermedia va a caducar](#la-intermedia-va-a-caducar);
- `hecate identities list` muestra hasta cuándo vale el certificado más reciente de cada identidad: la que lo tenga
  caducado necesita un token nuevo ([puesta en marcha](#puesta-en-marcha), paso 7).

## Hécate caída o sellada

**Cuándo:** `hecate serve` no está en marcha, o está sellada.

**Qué pasa mientras:**

- los componentes siguen con los secretos de su caché hasta 1 hora; después, lo que necesita un secreto falla con
  `internal`;
- sus certificados valen hasta que caducan, 3 días por defecto, y no se pueden renovar;
- sellada, la API responde `unavailable` a todo salvo al estado agregado, que dice `sealed: true` a las identidades con
  el permiso de estado.

**Pasos:** si está caída, [arráncala y desbloquéala](#arrancar-y-desbloquear-tras-un-reinicio-o-un-corte); si solo
está sellada, desbloquéala con `hecate unseal`.

**Cómo comprobarlo:** como tras un reinicio.

## Se pierde el disco

**Cuándo:** el disco o el directorio de datos se pierden o se estropean.

Hécate no tiene copias propias: lo que se puede hacer es una copia en frío de su directorio de datos y restaurarla. La
política común de copias de Cerbero (regla 3-2-1, cifrado y restauración probada en un entorno aislado, sin red hacia
casa) es una decisión [abierta](../../DECISIONS.md#abiertas).

**Qué necesitas:** una copia del directorio de datos y la frase o dos partes que valían cuando se hizo: la
configuración de sellado viaja en la copia.

**Hacer la copia:**

1. Para Hécate: SIGINT o SIGTERM a `hecate serve`.
2. Copia el directorio de datos entero conservando sus permisos (p. ej. `cp -Rp`) a un sitio que solo lea el usuario de
   Hécate.
3. Arranca `hecate serve` y desbloquéala.

**Restaurar:**

1. Con Hécate parada, pon la copia en el lugar del directorio de datos, con permisos `0700`.
2. Arranca `hecate serve` y desbloquéala con la frase o las partes de la copia.

**Cómo comprobarlo:** `hecate status` dice `state: unsealed`, y los secretos se vuelven a resolver.

- Lo posterior a la copia se pierde: secretos y versiones nuevos, cambios de política, tokens, altas y renovaciones,
  y las filas del registro de accesos. Un componente con un certificado emitido después necesita un token nuevo.
- Sin la frase o dos partes, la copia no descifra ningún secreto, a propósito. Pero lleva en claro la clave propia de
  Hécate ([riesgo aceptado](../../DECISIONS.md#riesgos-aceptados)): se guarda tan protegida como los datos.

## Se olvida la frase

**Cuándo:** Hécate está sellada y nadie recuerda la frase. Mientras siga desbloqueada no pasa nada: basta con fijar una
frase nueva (paso 2).

**Qué necesitas:** dos de las tres partes de recuperación.

**Pasos:**

1. Desbloquea con las partes: `hecate unseal --shares`.
2. Fija una frase nueva demostrándolo con las partes: `hecate passphrase change --with-shares`.
3. Si alguna parte pudo quedar expuesta al teclearla, reparte otras: `hecate shares rekey` (las anteriores dejan de
   valer).

**Cómo comprobarlo:** `hecate status` ya no muestra la recomendación de cambiar la frase, y el siguiente desbloqueo
funciona con la frase nueva.

## Se pierden la frase y las partes

**Cuándo:** no queda ni la frase ni dos partes. Los datos de Hécate no se pueden descifrar, ni desde las copias: es el
precio de que nadie más pueda hacerlo.

**Qué necesitas:** los valores de los secretos de nuevo (lo prudente es rotarlos en cada proveedor) y acceso a cada
componente para volver a darlo de alta.

**Pasos:** Hécate no tiene una orden para borrarlo todo; se empieza de cero a mano.

1. Para Hécate y borra su directorio de datos.
2. Sigue la [puesta en marcha](#puesta-en-marcha) entera: inicializar, desbloquear y crear una CA nueva.
3. Vuelve a guardar los secretos, a conceder los permisos y a dar de alta cada componente con un token nuevo.

**Cómo comprobarlo:** como en la puesta en marcha, para cada componente.

## La intermedia va a caducar

**Cuándo:** `hecate status` muestra `intermediate days left: <n>` (le quedan menos de 30 días), o ya caducó. Con la
intermedia caducada, Hécate no emite certificados ni crea tokens, la API no escucha y los componentes no pueden
renovar: sus certificados y el de Hécate caducaron con ella ([identidades](identidades.md#la-ca-en-dos-niveles)).

**Qué necesitas:** Hécate desbloqueada, la copia de la clave de la CA raíz y la frase de la CA. Si ya caducó, también
acceso a cada componente para volver a darlo de alta.

**Pasos:**

1. Renueva la intermedia: `hecate ca renew-intermediate --root-key <copia>`, y la frase de la CA cuando la pida.
2. Si ya había caducado, da de alta otra vez cada componente: `hecate tokens create <identidad>`, y el componente se
   da de alta con el token nuevo.

**Cómo comprobarlo:**

- la orden escribe `renewed intermediate version <n> until <fecha>`, y `hecate status` deja de mostrar
  `intermediate days left`;
- renovada a tiempo, los componentes conservan sus certificados; los nuevos los firma la intermedia nueva, y la
  anterior solo firma la lista de revocación de los suyos hasta que caduca el último;
- si ya había caducado, Hécate renueva su certificado al momento: el log de `hecate serve` vuelve a decir
  `the API listens` y `hecate status` muestra la serie nueva. Cada componente aparece en `hecate identities list` con
  su certificado nuevo tras darse de alta.

## CA comprometida o perdida

**Cuándo:** la clave de la intermedia pudo quedar expuesta, o se pierden la copia de la clave de la CA raíz o su frase
(sin ellas no se puede renovar la intermedia).

**Qué necesitas:** Hécate desbloqueada, la frase de desbloqueo actual (o dos partes), una frase de la CA nueva, dos
soportes para la copia nueva y acceso a cada componente para volver a darlo de alta.

**Pasos:**

1. Sustituye la CA: `hecate ca replace --out <fichero>` (con `--with-shares` si la prueba es con partes). Fija la frase
   de la CA nueva y verifica la copia como en la puesta en marcha (paso 4). Hécate destruye las claves de las
   intermedias anteriores y solo acepta la CA nueva.
2. Para cada componente: `hecate tokens create <identidad>`, y el componente se da de alta otra vez con el token
   nuevo, que lleva la huella de la CA nueva. Los tokens creados antes de sustituirla ya no valen.

**Cómo comprobarlo:**

- `hecate status` muestra la huella de la CA nueva;
- un componente sin el alta nueva no conecta con Hécate, que se presenta con una CA raíz que él no reconoce (y su
  certificado tampoco vale para ella); tras darse de alta de nuevo vuelve a funcionar.

## Clave de un componente comprometida

**Cuándo:** la clave TLS o la de firma de llamadas de un componente pudo quedar expuesta, o el estado cuenta una
renovación sospechosa (en ese caso, Hécate ya revocó con urgencia sus certificados).

**Qué necesitas:** Hécate desbloqueada.

**Pasos:**

1. Mira sus certificados: `hecate identities get <identidad>`.
2. Corta el acceso:
   - un certificado concreto: `hecate certificates revoke <serie> --urgent`;
   - toda la identidad: `hecate identities disable <identidad> --urgent`, que además revoca con urgencia sus
     certificados válidos.
3. Cuando el componente esté limpio, con claves nuevas: si se deshabilitó, `hecate identities enable <identidad>`;
   después, `hecate tokens create <identidad>` y el componente se da de alta otra vez.

**Cómo comprobarlo:**

- `hecate identities get <identidad>` muestra sus certificados como `revoked`;
- su serie está en la lista de revocación, que los demás componentes recogen en menos de 5 minutos;
- deshabilitada, `hecate identities list` la muestra como `disabled` y ya no renueva.

## Clave propia de Hécate comprometida

**Cuándo:** la clave del certificado de Hécate pudo quedar expuesta, p. ej. porque se filtró una copia de su base de
datos, que la guarda en claro.

**Qué necesitas:** Hécate desbloqueada.

**Pasos:**

1. Mira la serie de su certificado: `hecate status`, en la línea del certificado de Hécate.
2. Revócalo con urgencia: `hecate certificates revoke <serie> --urgent`. Hécate lo reemite al momento con otra clave.

**Cómo comprobarlo:** la orden escribe `reissued <serie nueva>`, `hecate status` muestra la serie nueva, y la anterior
entra en la lista de revocación. Los componentes siguen hablando con Hécate, porque confían en la CA raíz.

## Clave de ámbito comprometida

**Cuándo:** la clave de ámbito de una identidad pudo quedar expuesta: los secretos de esa identidad quedan expuestos.

Las claves de ámbito no se rotan: su rotación es una decisión [abierta](../../DECISIONS.md#abiertas). Lo que se puede
hacer con lo que hay:

1. Rota cada secreto de esa identidad en su proveedor y guarda la versión nueva: `hecate secrets put <ruta>`.
2. Revoca las versiones expuestas, `hecate secrets revoke <ruta> --version <n>`, y destrúyelas,
   `hecate secrets destroy <ruta> --version <n>`.
3. Las versiones nuevas se cifran con la misma clave de ámbito. Si quien la tiene puede llegar también a una copia de
   la base de datos, la única forma de tener una clave de ámbito nueva es una identidad nueva para el componente:
   guarda sus secretos con `hecate secrets create` bajo el nombre nuevo, concede de nuevo sus permisos, dale de alta con
   `hecate tokens create <identidad nueva>`, deshabilita la anterior con `hecate identities disable <identidad>` y
   revoca y destruye sus secretos.

**Cómo comprobarlo:** `hecate secrets get <ruta>` muestra las versiones expuestas como `destroyed` y la nueva como
`active`, y el componente resuelve las nuevas.

## Relacionado

- [Hécate](../hecate.md): el documento de alto nivel, con el comportamiento ante fallos.
- [CLI y configuración](cli.md): cada orden que usan los procedimientos.
- [Sellado](sellado.md) e [identidades](identidades.md): el diseño que hay detrás de cada procedimiento.
- [DECISIONS](../../DECISIONS.md#abiertas): lo que sigue abierto.
