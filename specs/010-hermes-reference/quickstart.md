# Quickstart: comprobar la documentación de Hermes

Guía de validación del cambio 010. Las comprobaciones hacen de pruebas: las que miran los documentos nuevos (1, 2, 3, 6
y el recuento de palabras de 11) fallan sobre `main`, y todas tienen que pasar al terminar (research R11). Todas se
ejecutan desde la raíz del repositorio. Los scripts del recorrido viven fuera del repositorio: el cambio no añade código
(FR-001).

## 1. Las órdenes de la CLI (FR-007, SC-003)

```bash
HERMES_ADMIN_DIR=/tmp uv run --package cerbero-hermes python - <<'EOF'
import pathlib, re, subprocess
def orders(*args):
    help_text = subprocess.run(["hermes", *args, "--help"], capture_output=True, text=True).stdout
    return re.findall(r"^    ([a-z][a-z-]*)  ", help_text, re.MULTILINE)
finals = [f"{top} {sub}".strip() for top in orders() for sub in (orders(top) or [""])]
docs = [pathlib.Path("docs/hermes.md"), *sorted(pathlib.Path("docs/hermes").glob("*.md")), pathlib.Path("hermes/README.md")]
reference = pathlib.Path("docs/hermes/cli.md").read_text() if pathlib.Path("docs/hermes/cli.md").exists() else ""
for order in finals:
    if f"hermes {order}" not in reference: print("falta:", order)
named = {m for doc in docs if doc.exists() for m in re.findall(r"`hermes ([a-z][a-z-]*(?: [a-z][a-z-]*)?)", doc.read_text())}
for order in sorted(named):
    if order not in finals and order.split()[0] not in [f.split()[0] for f in finals if " " not in f]: print("sobra:", order)
EOF
```

**Esperado:** no imprime nada. Las seis órdenes finales están en la referencia, y ningún documento de Hermes nombra una
orden que no exista en la ayuda (se admiten argumentos detrás de la orden).

## 2. Las variables de entorno (FR-007, SC-003)

```bash
python3 - <<'EOF'
import pathlib, re
variables = sorted(set(re.findall(r'"(HERMES_[A-Z_]+)"', pathlib.Path("hermes/src/hermes/infrastructure/settings.py").read_text())))
reference = pathlib.Path("docs/hermes/cli.md").read_text() if pathlib.Path("docs/hermes/cli.md").exists() else ""
for variable in variables:
    if variable not in reference: print("falta:", variable)
docs = [pathlib.Path("docs/hermes.md"), *sorted(pathlib.Path("docs/hermes").glob("*.md")), pathlib.Path("hermes/README.md")]
named = {v for doc in docs if doc.exists() for v in re.findall(r"HERMES_[A-Z_]+", doc.read_text())}
for variable in sorted(named - set(variables)): print("sobra:", variable)
EOF
```

**Esperado:** no imprime nada. Las once variables están en la referencia con su valor por defecto y sus límites, que se
comparan a mano con `settings.py`, y ningún documento nombra otra.

## 3. Las operaciones de la API (FR-006, SC-004)

```bash
python3 - <<'EOF'
import json, pathlib, re
api = json.load(open("shared/contracts/src/cerbero_contracts/openapi/hermes.v1.json"))["paths"]
def text(name):
    path = pathlib.Path(name)
    return path.read_text() if path.exists() else ""
guide = text("docs/hermes/integracion.md")
for path in api:
    if path not in guide: print("falta:", path)
templates = [re.compile("^" + "[^/]+".join(map(re.escape, re.split(r"\{[^}]+\}", path))) + "$") for path in api]
uses = guide + text("docs/hermes/cli.md") + text("docs/hermes/operacion.md")
for path in sorted(set(re.findall(r"/(?:chat|v1)/[A-Za-z0-9{}_/-]*[A-Za-z0-9}_]", uses))):
    if not any(template.match(path) for template in templates): print("sobra:", path)
EOF
```

**Esperado:** no imprime nada: las tres operaciones están en la guía de integración, con sus parámetros, y ninguna
referencia de uso nombra otra ruta de Hermes; una ruta con un valor concreto, como el nombre de una instancia, vale si
encaja con la de su operación.

## 4. Enlaces y anclas (FR-016, FR-017)

```bash
python3 - docs/hermes.md $(find docs/hermes -name '*.md' 2>/dev/null) hermes/README.md README.md DECISIONS.md ARCHITECTURE.md GLOSSARY.md <<'EOF'
import re, sys, pathlib
def slug(title):
    title = re.sub(r'[`*]', '', title.strip().lower())
    return re.sub(r'\s', '-', re.sub(r'[^\w\s-]', '', title))
def prose(path):
    return re.sub(r'```.*?```', '', path.read_text(), flags=re.S)
def anchors(path):
    return {slug(line.lstrip('#')) for line in prose(path).splitlines() if line.startswith('#')}
for name in sys.argv[1:]:
    source = pathlib.Path(name)
    if not source.exists(): print('no existe:', name); continue
    for target in re.findall(r'\]\(([^)\s]+)\)', prose(source)):
        if target.startswith(('http://', 'https://', 'mailto:')): continue
        file, _, anchor = target.partition('#')
        path = (source.parent / file).resolve() if file else source.resolve()
        if not path.exists(): print(f'{name}: no existe {target}'); continue
        if anchor and path.suffix == '.md' and anchor not in anchors(path): print(f'{name}: no existe el ancla {target}')
EOF
```

**Esperado:** no imprime nada. Sobre `main` tampoco imprime nada: vigila que la reorganización no deje enlaces rotos, en
especial los de DECISIONS a los apartados de `docs/hermes.md`.

## 5. Sin código ni specs fusionadas (FR-001, FR-019)

```bash
git diff --name-only main -- . ':(exclude)*.md'; git ls-files --others --exclude-standard | grep -v '\.md$'
```

```bash
git diff --name-only main -- specs | grep -v '^specs/010-hermes-reference/'; git ls-files --others --exclude-standard specs | grep -v '^specs/010-hermes-reference/'
```

**Esperado:** las dos órdenes no imprimen nada: solo cambian o se añaden documentos Markdown, también entre los ficheros
nuevos, y en `specs/` solo cambia la carpeta de este cambio.

## 6. El estado y los enlaces de DECISIONS (FR-013, FR-016)

```bash
grep -n "hermes.md#" DECISIONS.md; grep -c "docs/hermes/llamadas.md\|docs/hermes/comunicacion.md" DECISIONS.md; sed -n '/Estado del Proyecto/,/^---/p' README.md | grep -c "docs/hermes.md"
```

**Esperado:** la primera búsqueda no imprime nada; DECISIONS enlaza tres veces a los documentos nuevos (las tasks,
`chat.ask` y los grupos de contactos); y el estado del README enlaza a `docs/hermes.md`.

## 7. Nada se pierde (FR-004, SC-002)

A mano, con el mapa de [research.md](research.md#r3-qué-va-a-cada-documento): cada apartado del `docs/hermes.md` de
`main` (`git show main:docs/hermes.md`) está en su destino, y cada dato que cambia por no coincidir con el código está
en la guía de revisión (research R12).

## 8. Ningún documento lleva el token del bot de pruebas (FR-014, constitución V)

```bash
( f=hermes/deploy/.secrets/telegram-test.env; [ -f "$f" ] || { echo "falta: $f"; exit 0; }; set -a; . "./$f"; set +a; [ -n "$TELEGRAM_TEST_BOT_TOKEN" ] || { echo "falta: TELEGRAM_TEST_BOT_TOKEN"; exit 0; }; grep -rlF -- "$TELEGRAM_TEST_BOT_TOKEN" docs hermes/README.md README.md DECISIONS.md ARCHITECTURE.md GLOSSARY.md specs/010-hermes-reference; true )
```

**Esperado:** no imprime nada. La orden nunca escribe el token: solo los nombres de los ficheros que lo contuvieran, o
`falta:` si no existe el fichero del bot de pruebas, que crea el propietario.

## 9. El recorrido (FR-011, SC-005, SC-006)

Con el compose de `hermes/deploy/`, siguiendo los procedimientos de `docs/hermes/operacion.md` tal como están escritos,
y cada orden ejecutada como dice su apartado del compose de pruebas.

| Procedimiento | Con | Esperado |
|---|---|---|
| Puesta en marcha (happy path) | El bot de pruebas, con su token de `hermes/deploy/.secrets/telegram-test.env` | Hermes dado de alta y listo, la configuración aplicada, el canal del propietario verificado con «Canal verificado.» tras enviar él el código, el acceso concedido y un envío del cliente de referencia a través de Envoy que llega a su Telegram |
| Conceder y retirar el acceso | El fake | `auth` antes de conceder, el envío en la llamada siguiente a la concesión y `auth` como mucho a los 240 s de retirarlo |
| Verificar un canal | El fake | El código, el mensaje en su bandeja de entrada y la respuesta en la de salida |
| Hécate sellada o caída | El fake | Los envíos siguen con la caché y las copias; al desbloquearla o arrancarla, todo sigue sin reiniciar Hermes |
| PostgreSQL caído | El fake | `not ready: database unavailable` y los envíos `unavailable`; al arrancarla, Hermes vuelve a estar listo solo |
| La identidad de Hermes revocada | El fake | Tras la revocación urgente, Hermes sigue `ready` y `hermes enroll` responde `conflict`; parado, con su directorio de estado vacío y arrancado otra vez, `not ready: needs a new enrollment token`; con un token nuevo y `hermes enroll`, listo |
| Sin copia de la lista de acceso y Hécate caída | El fake | `not ready: access list not loaded` y los envíos `unavailable` |

**Rotar el token de un bot** y **una instancia que falla** se comprueban con mocks, como decidió el propietario: pasan
las pruebas con respuestas grabadas de Telegram, con las credenciales rechazadas y la prueba de conexión
(`hermes/tests/integration/telegram/` y `hermes/tests/integration/api/test_instance_test.py`), y las de la caché de
secretos (`hermes/tests/unit/application/test_secret_cache.py`); y cada paso de sus procedimientos usa una orden que
existe. **Restaurar una copia** se comprueba leyendo: cada paso existe, y lo que falta remite a DECISIONS.

**La identidad caducada o de otra CA** sigue los mismos pasos que la revocada, que es la que se recorre: se comprueba
leyendo que su procedimiento lleva a esos pasos y que cada orden existe.

## 10. El README de desarrollo (FR-012, SC-007)

Siguiendo solo `hermes/README.md`: la suite con `HECATE_BIN` y Docker, con el E2E, y las comprobaciones de calidad. Como
el cambio no toca código, el resultado es el de la feature 009 (866 pruebas).

## 11. Lectura y tiempo (SC-001, FR-015)

```bash
wc -w docs/hermes.md
```

```bash
grep -n -i -w -E 'hoy|todavía|por ahora|más adelante|en el futuro' docs/hermes.md $(find docs/hermes -name '*.md' 2>/dev/null) hermes/README.md
```

**Esperado:** el documento de alto nivel no pasa de 2.500 palabras, unos 10 minutos de lectura, y la búsqueda no imprime
nada: ningún documento de Hermes usa palabras de tiempo.
