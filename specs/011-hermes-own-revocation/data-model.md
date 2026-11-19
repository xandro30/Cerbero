# Data Model: Hermes nota la revocación urgente de su propio certificado

Lo que el arreglo 011 añade al modelo de Hermes: una marca en memoria en su servicio de identidad. **No hay ficheros,
tablas ni migraciones nuevos**: el directorio de estado, el del proxy y la base de Hermes no cambian. Decisiones en
[research.md](research.md) (R3, R4, R6 y R7).

## La validez de la identidad de Hermes

El servicio de identidad ya guarda en memoria la identidad vigente (su certificado, su cadena y sus claves, cargados
del directorio de estado) y si Hécate presenta otra CA raíz. El arreglo cambia la marca de rechazo:

| Dato | Hoy | Con el arreglo |
|---|---|---|
| La marca de rechazo | Un booleano que pone la renovación rechazada | La serie de la identidad que Hécate rechazó, o ninguna; la pone cualquier llamada rechazada (R3) |
| Dónde vive | En memoria | En memoria: un reinicio la pierde (FR-007) |
| Quién la quita | El alta | El alta, que trae otra identidad, o un reinicio |

- **La identidad no vale** (`needs a new enrollment token`) sin identidad, con el certificado caducado, con su serie
  en la lista de revocación, con la CA sustituida o con su serie marcada.
- **Una llamada a Hécate con la identidad** sale solo si su serie no está marcada; si lo está, falla con `auth` sin
  salir (FR-008). La CA sustituida no bloquea las llamadas (FR-009).

### Transiciones

```text
identidad válida ──auth a una llamada con ella, si sigue siendo la vigente──▶ identidad rechazada
identidad rechazada ──alta con un token nuevo──▶ identidad válida (otra serie, sin marca)
identidad rechazada ──reinicio──▶ sin marca; la primera llamada con ella vuelve a comprobarla
identidad rechazada ──llamada a Hécate con ella──▶ auth sin salir de Hermes
identidad ya sustituida por un alta ──auth atrasado de una llamada suya──▶ no cuenta (FR-004)
```

## Lo que no cambia

- **El directorio de estado y el del proxy:** el alta escribe una generación nueva, con su clave de firma y sus
  metadatos, borra la renovación en curso y quita las generaciones viejas, como hoy; las publicaciones de la identidad
  nueva sustituyen a las copias anteriores (R7).
- **La base de Hermes:** la configuración, la verificación de los canales, la idempotencia, los `jti` y la auditoría
  siguen iguales tras el alta (FR-006).
- **La caché de secretos:** sigue solo en memoria, con sus tiempos blando y duro; ante una identidad rechazada se
  comporta como hoy ante un `auth` de Hécate (R4). Lo único nuevo es que, tras el alta, `hermes serve` vuelve a pedir
  la contraseña de la base, que pudo quitar (R7).
