# Quickstart: validar el cambio 004

Comprobaciones de este cambio de documentación. Hacen de pruebas ([research.md](research.md), R9): antes de escribir,
las que miden algo pendiente fallan; al terminar, tienen que pasar todas. Se ejecutan desde la raíz del repositorio, en
la rama `chore/hecate-orthros-rust`, con `main` al día en local y antes del commit, cuando los ficheros nuevos aún no
están versionados.

Los requisitos están en [spec.md](spec.md) y los formatos, en [data-model.md](data-model.md).

## V1. Ni código ni configuración (FR-001, SC-003)

```bash
git diff --name-only main | grep -v -E '\.md$|^LICENSE-EXCEPTION$'
git ls-files --others --exclude-standard | grep -v -E '\.md$|^LICENSE-EXCEPTION$'
```

**Esperado:** ninguna salida, antes y después.

## V2. La migración no se hace en esta rama (FR-008, SC-004)

```bash
grep -rn --include='*.md' 'chore/hecate-rust' . | grep -v -E '^(\./)?specs/'
```

**Antes:** el Sync Impact Report y la entrada 1.2.0 de la constitución. **Después:** ninguna salida.

## V3. Ningún doc describe código en Rust (FR-016, SC-004)

```bash
grep -n -i -E 'cargo|crate|rust-toolchain' README.md ARCHITECTURE.md docs/*.md hecate/README.md orthros/README.md orthros/*/README.md
```

**Esperado:** ninguna salida, antes y después. La constitución y los ADRs 0005 a 0007 hablan de Cargo y de crates como
reglas y decisiones, no como código que exista.

## V4. La excepción de Shamir (FR-015)

```bash
grep -n -i 'librerías auditadas' README.md ARCHITECTURE.md DECISIONS.md docs/hecate.md | grep -v -i -E 'sin auditoría|riesgo aceptado'
grep -n 'Riesgos aceptados.*Shamir' docs/hecate.md
```

**Antes:** la primera da siete líneas (README 181, ARCHITECTURE 196, DECISIONS 93 y 108, doc de Hécate 18, 23 y 355) y
la segunda, ninguna. **Después:** la primera, ninguna, porque cada afirmación lleva en la misma línea la excepción de
Shamir; la segunda, el resumen de riesgos del apartado 12 del doc de Hécate.

## V5. Terminología (FR-018)

```bash
grep -n -i -E 'feature de spec kit|cada feature|NNN-feature|dentro de una feature|al cerrar una feature' .specify/memory/constitution.md README.md
```

**Antes:** cinco reglas de la constitución, dos sitios del README y las entradas 1.0.0 y 1.1.0 del historial.
**Después:** solo esas dos entradas del historial, que son el registro de su fecha (research R8).

## V6. Reglas de flujo (FR-005, FR-006, FR-019, SC-005)

```bash
grep -n -E 'refactor|conversación|pausad|ya fusionad|número siguiente' .specify/memory/constitution.md
```

**Antes:** ninguna regla. **Después:** las reglas de «Fuentes de verdad», «Orden», «Ramas», «Una cosa a la vez» y
«Commits» (research R3), además de la entrada 1.2.0 del historial.

Lectura guiada (SC-005): con la constitución sola, sin preguntar al propietario, se responde para la migración de
Hécate:

| Pregunta | Respuesta esperada |
|---|---|
| ¿Pasa por Spec Kit? | Sí, con el ciclo completo; un plan aprobado en el chat no basta |
| ¿Qué rama? | `refactor/<nombre>`; el número no va en el nombre |
| ¿Qué carpeta? | `specs/005-<nombre>`: el siguiente al mayor de `main` (004) y de los pausados (003) |
| ¿Qué commit? | Squash `refactor:` |
| ¿Y la feature 003? | Pausada en DECISIONS hasta que se fusione la migración |

## V7. DECISIONS (FR-012, FR-013)

```bash
sed -n '/^## Cambios pausados/,$p' DECISIONS.md
grep -n 'Python no garantiza borrar la raíz' DECISIONS.md
```

**Antes:** la primera no da salida. **Después:** la sección con la fila de la feature 003. La segunda da el riesgo de
memoria, que sigue describiendo Python, antes y después.

## V8. ADRs y registros históricos (FR-009, FR-011, FR-019)

```bash
git diff --stat main -- docs/adr/0001-monorepo-uv-workspaces.md docs/adr/0003-persistencia-hecate-sqlite.md docs/adr/0004-pruebas-pytest-hypothesis.md specs/001-hecate-bootstrap specs/002-hecate-secrets
git diff main -- docs/adr/0002-lenguajes-por-proyecto.md | grep -E '^[+-][^+-]'
grep -n -E '0001|consolid' docs/adr/0005-rust-para-orthros-y-hecate.md
```

**Esperado:** la primera, sin salida; la segunda, solo la línea de estado del 0002. **Antes**, la tercera no da salida;
**después**, la viñeta «Repositorio» enlaza el 0001 y otra viñeta lista lo que se actualiza al consolidar la
migración: el README, el README de Orthros, el doc de Hécate y el riesgo de memoria de DECISIONS.

## V9. Enlaces (FR-017, SC-006)

Comprueba los enlaces relativos de los `.md` modificados o nuevos, y las anclas a secciones con la forma que les da
GitHub:

```bash
.venv/bin/python -I - <<'EOF'
import pathlib, re, subprocess

def run(*args):
    return subprocess.run(args, capture_output=True, text=True, check=True).stdout.split()

def slug(heading):
    return re.sub(r'[^\w\- ]', '', heading.strip().lower()).replace(' ', '-')

files = set(run('git', 'diff', '--name-only', 'main', '--', '*.md'))
files |= set(run('git', 'ls-files', '--others', '--exclude-standard', '--', '*.md'))
broken = []
for name in sorted(files):
    source = pathlib.Path(name)
    for target in re.findall(r'\]\(([^)\s]+)\)', source.read_text()):
        if re.match(r'[a-z]+:', target):
            continue
        path, _, anchor = target.partition('#')
        dest = source.parent / path if path else source
        if not dest.exists():
            broken.append(f'{name}: {target}')
        elif anchor and dest.suffix == '.md':
            headings = {slug(h) for h in re.findall(r'^#{1,6} (.+)$', dest.read_text(), re.M)}
            if anchor not in headings:
                broken.append(f'{name}: {target}')
print('\n'.join(broken) or 'OK')
EOF
```

**Esperado:** `OK`. Si no, la lista de enlaces rotos con su fichero.

## V10. Enmienda lista para el commit (FR-007)

```bash
head -1 .specify/memory/constitution.md
grep -n '^\*\*Version\*\*' .specify/memory/constitution.md
```

**Antes:** la primera línea abre el Sync Impact Report (`<!--`). **Después:** `# Cerbero Constitution`, y la versión,
1.2.0.

## V11. Lectura de coherencia (SC-001, SC-002)

Se leen en este orden, anotando cualquier contradicción: la constitución, el índice de ADRs y los ADRs 0005 a 0008,
DECISIONS, el README, ARCHITECTURE y el doc de Hécate. No cuentan los ADRs ya fusionados, las specs fusionadas ni los
textos que describen la implementación actual (SC-001). Cada decisión del cambio de lenguaje tiene su ADR con sus
cuatro partes, en el índice y en DECISIONS (SC-002).
