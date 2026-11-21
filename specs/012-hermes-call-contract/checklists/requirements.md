# Specification Quality Checklist: El contrato de llamada de Hermes: repetir con la misma clave y el `kid` del token

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-19
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

- Items marked incomplete require spec updates before `/speckit-clarify` or `/speckit-plan`
- El `[NEEDS CLARIFICATION]` de FR-002, la regla que consigue que el reintento de Hades llegue a enviar, quedó resuelto
  el 2026-11-19 con el camino 1 (Clarifications); su respuesta fijó también los casos límite de las peticiones a la vez
  y de la misma clave con otro cuerpo, y la «respuesta guardada» de las entidades.
- Al integrarla salió otro punto, el `[NEEDS CLARIFICATION]` de FR-004: qué responde Hermes, y qué guarda la clave,
  ante un fallo inesperado mientras llama al proveedor. Hoy responde `internal`, que la guía de integración manda
  repetir con la misma clave, pero lo guarda en la clave, y la repetición recibe el mismo `internal` durante 24 horas.
  Quedó resuelto el 2026-11-19 (Clarifications): `timeout` con `outcome_unknown`, guardado como un resultado
  desconocido; su respuesta añadió el escenario 4 de la historia 3 y el caso límite del fallo inesperado antes del
  proveedor.
- Los nombres `Idempotency-Key`, `provider_unavailable`, `rate_limited`, `timeout`, `outcome_unknown`, `retryable`,
  `conflict`, `internal`, `auth` y `kid` aparecen porque son el contrato vigente de Hermes (Assumptions), como en las
  specs de la 008, la 009 y la 011; no son una elección de tecnología de este cambio.
