# Quickstart: comprobar la documentación de Hécate

Guía de validación del cambio 007. Las comprobaciones hacen de pruebas: sobre `main` fallan, y tienen que pasar al
terminar (research R11). Todas se ejecutan desde la raíz del repositorio. Los scripts del recorrido viven fuera del
repositorio: el cambio no añade código (FR-001).

## 1. Las órdenes de la CLI (FR-006, SC-003)

```bash
cargo build --manifest-path hecate/Cargo.toml -p hecate-cli --bin hecate
```

```bash
H=hecate/target/debug/hecate; L=$(mktemp); commands() { "$@" --help | sed -n '/^Commands:/,/^$/p' | awk 'NR>1 && $1!="help" {print $1}'; }; for top in $(commands $H); do if [ -z "$(commands $H $top)" ]; then echo "hecate $top"; else for s in $(commands $H $top); do echo "hecate $top $s"; done; fi; done > $L; while read -r cmd; do grep -qF "$cmd" docs/hecate/cli.md 2>/dev/null || echo "falta: $cmd"; done < $L; cat docs/hecate.md $(find docs/hecate -name '*.md' 2>/dev/null) | grep -oE '`hecate [a-z][a-z-]*( [a-z][a-z-]*)?' | tr -d '`' | sort -u | while read -r cmd; do grep -qxF "$cmd" $L || grep -qxF "${cmd% *}" $L || echo "sobra: $cmd"; done
```

**Esperado:** no imprime nada. Las 32 órdenes finales están en la referencia, y ningún documento de Hécate nombra una
orden que no exista en la ayuda (se admiten argumentos detrás de la orden).

## 2. Las variables de entorno (FR-006, SC-003)

```bash
for v in $(grep -ohE 'HECATE_[A-Z_]+' hecate/crates/infrastructure/src/config.rs | sort -u); do grep -qF "$v" docs/hecate/cli.md 2>/dev/null || echo "falta: $v"; done; cat docs/hecate.md $(find docs/hecate -name '*.md' 2>/dev/null) hecate/README.md | grep -ohE 'HECATE_[A-Z_]+' | sort -u | while read -r v; do grep -qF "$v" hecate/crates/infrastructure/src/config.rs || echo "sobra: $v"; done
```

**Esperado:** no imprime nada. Las seis variables están en la referencia con su valor por defecto y sus límites, que se
comparan a mano con `config.rs` (`DEFAULT_*` y los rangos de `bounded`), y ningún documento nombra otra.

## 3. Las operaciones de la API (FR-005, SC-004)

```bash
python3 -c "
import json, re, pathlib
api = json.load(open('shared/contracts/src/cerbero_contracts/openapi/hecate.v1.json'))['paths']
doc = pathlib.Path('docs/hecate/integracion.md')
text = doc.read_text() if doc.exists() else ''
for path in api: print('falta:', path) if path not in text else None
for path in sorted(set(re.findall(r'/v1/[A-Za-z0-9:{}/_-]+', text)) - set(api)): print('sobra:', path)
"
```

**Esperado:** no imprime nada.

## 4. Enlaces y anclas (FR-015, SC-009)

```bash
python3 - docs/*.md $(find docs/hecate -name '*.md' 2>/dev/null) hecate/README.md README.md DECISIONS.md ARCHITECTURE.md GLOSSARY.md .specify/memory/constitution.md <<'EOF'
import re, sys, pathlib
def slug(title):
    title = re.sub(r'[`*_]', '', title.strip().lower())
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

**Esperado:** no imprime nada.

## 5. Sin código (FR-001, SC-010)

```bash
git diff --name-only main -- . ':(exclude)*.md'; git ls-files --others --exclude-standard | grep -v '\.md$'
```

**Esperado:** no imprime nada: solo cambian o se añaden documentos Markdown, también entre los ficheros nuevos.

## 6. El estado y la constitución (FR-009, FR-010, FR-011, FR-019)

```bash
grep -c "Diseñado, no implementado" DECISIONS.md; grep -n "Despliegue en el home lab" DECISIONS.md; sed -n '/Estado del Proyecto/,/^---/p' README.md | grep -n "docs/hecate.md"; grep -n '^\*\*Version\*\*: 1.2.1' .specify/memory/constitution.md; grep -n "1.2.1" .specify/memory/constitution.md | head -3
```

**Esperado:**

- DECISIONS tiene siete filas «Diseñado, no implementado»: la del coche escoba de Caronte, que ya estaba, y las seis de
  Hécate; y la del despliegue en el home lab;
- el README enlaza a `docs/hecate.md` desde su estado (no basta la lista de componentes, que ya lo enlaza);
- la constitución está en la 1.2.1 y su historial tiene la entrada de la aclaración, con qué, por qué, alternativas y a
  qué afecta.

## 7. Nada se pierde (FR-004, SC-002)

A mano, con el mapa de [research.md](research.md#r3-qué-va-a-cada-documento): cada apartado del `docs/hecate.md` de
`main` (`git show main:docs/hecate.md`) está en su destino, y cada dato que cambia por no coincidir con el código está en
la guía de revisión (research R12).

## 8. El recorrido (FR-007, SC-006)

Con el binario release y el ejemplo `component`, compilados como en el apartado 5 del quickstart de la feature 006, en
un directorio temporal de ruta corta (`mktemp -d`, para que quepa la ruta del socket) y con
`HECATE_KDF_PROFILE=moderate`. Se siguen los procedimientos de `docs/hecate/operacion.md` tal como están escritos, con
un driver para las órdenes interactivas: la confirmación y las partes de `hecate init`, las ceremonias de la CA
(`ca create`, `ca renew-intermediate` y `ca replace`, con la frase de la CA y la copia de la clave raíz), `unseal` con
partes y `passphrase change` con su prueba.

| Procedimiento | Esperado |
|---|---|
| Puesta en marcha | Hécate inicializada y desbloqueada, la CA creada, el componente de prueba dado de alta y resolviendo un secreto que se le ha guardado |
| Arrancar y desbloquear tras un reinicio | Tras parar y arrancar `hecate serve`, `hecate status` dice `sealed`; tras desbloquear, `unsealed`, y el componente vuelve a resolver |
| Se pierde el disco | Con Hécate parada se copia el directorio de datos; tras borrarlo y restaurar la copia, Hécate arranca, se desbloquea y el secreto se sigue resolviendo |
| Se olvida la frase | Se desbloquea con dos partes, se fija una frase nueva y se desbloquea con ella |
| Clave de un componente comprometida | La revocación urgente pone su serie en la lista de revocación; deshabilitada, la identidad ya no renueva |
| Clave propia de Hécate comprometida | Tras la revocación urgente, Hécate presenta un certificado nuevo y el componente sigue hablando con ella |
| La intermedia va a caducar | `hecate ca renew-intermediate` con la copia de la clave raíz deja una intermedia nueva activa |
| CA comprometida o perdida | `hecate ca replace` deja una CA nueva, y el componente solo vuelve a hablar con Hécate tras darse de alta con un token nuevo |

Lo que no tiene un procedimiento completo (perder la frase y las partes, o una clave de ámbito comprometida) se
comprueba leyendo: cada paso que propone existe en la CLI, y lo que falta remite a DECISIONS. «Hécate caída o
sellada» se comprueba con la fila del reinicio. El recorrido de la puesta en marcha comprueba también las variables y
los requisitos del sistema que da la referencia de la CLI.

## 9. El README de desarrollo (FR-008, SC-008)

Siguiendo solo `hecate/README.md`: compila, pasa la suite en macOS, pasa la suite en el contenedor Linux y pasa las
comprobaciones de calidad. Como el cambio no toca código, el resultado es el de la feature 006 (840 pruebas).

## 10. Lectura y tiempo (SC-001, FR-013)

```bash
wc -w docs/hecate.md
```

```bash
grep -n -i -w -E 'hoy|todavía|por ahora|más adelante|en el futuro' docs/hecate.md $(find docs/hecate -name '*.md' 2>/dev/null) hecate/README.md
```

**Esperado:** el documento de alto nivel no pasa de 2.500 palabras, unos 10 minutos de lectura (sobre `main` tiene
6.486), y la búsqueda no imprime nada: ningún documento de Hécate usa palabras de tiempo.
