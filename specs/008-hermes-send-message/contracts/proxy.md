# Envoy delante de Hermes

Lo que hace el proxy y lo que Hermes lee de él. La configuración de las pruebas vive en `hermes/deploy/envoy/envoy.yaml`
y cumple este contrato; cómo corren los dos en el home lab sigue abierto en DECISIONS.

## Lo que hace Envoy

| Qué | Cómo | Requisito |
|---|---|---|
| Escucha con el certificado de Hermes | TLS 1.3 (mínimo y máximo), con el certificado y la clave de `HERMES_PROXY_DIR/current/`, leídos por SDS de ficheros con `watched_directory` en `HERMES_PROXY_DIR` | FR-001 |
| Recarga el certificado renovado | Hermes escribe cada generación en `gen-<n>/` y sustituye el enlace `current` de forma atómica; Envoy lo recarga sin reiniciarse ni cortar las conexiones en curso | FR-002 |
| Pide el certificado de cliente sin exigirlo | `require_client_certificate: false`, con la CA raíz de `current/ca.crt` en el contexto de validación y `trust_chain_verification: ACCEPT_UNTRUSTED` | FR-003, historia 2 |
| Cada conexión presenta su certificado | `disable_stateless_session_resumption` y `disable_stateful_session_resumption` | FR-003 |
| Sustituye la cabecera del cliente | `forward_client_cert_details: SANITIZE_SET`, con `set_current_client_cert_details` `cert`, `chain` y `uri` | FR-002, historia 2 |
| Reenvía a Hermes | Por el socket Unix `HERMES_RUN_DIR/api.sock` (dirección `pipe`), en 0600 y sin puerto TCP | FR-001 |
| No ve nada más de Hermes | Solo monta `HERMES_PROXY_DIR` y `HERMES_RUN_DIR`; no ve el directorio de estado ni el de administración | FR-005 |

- Envoy corre con el mismo usuario que Hermes (UID 10001 en las pruebas): lee su clave TLS y abre su socket.
- Sus logs de acceso van a la salida estándar sin cabeceras: nunca el certificado ni el token.

Esquema de la configuración (API v3):

```yaml
node: { id: hermes-proxy, cluster: hermes }   # SDS exige el id y el cluster del nodo
static_resources:
  listeners:
    - name: hermes_api
      address: { socket_address: { address: 0.0.0.0, port_value: 8443 } }
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              require_client_certificate: false
              disable_stateless_session_resumption: true
              disable_stateful_session_resumption: true
              common_tls_context:
                tls_params: { tls_minimum_protocol_version: TLSv1_3, tls_maximum_protocol_version: TLSv1_3 }
                alpn_protocols: ["h2", "http/1.1"]
                tls_certificate_sds_secret_configs:
                  - name: hermes_certificate
                    sds_config: { path_config_source: { path: /etc/envoy/sds/certificate.yaml } }
                validation_context_sds_secret_config:
                  name: cerbero_root
                  sds_config: { path_config_source: { path: /etc/envoy/sds/validation.yaml } }
          filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: hermes
                forward_client_cert_details: SANITIZE_SET
                set_current_client_cert_details: { cert: true, chain: true, uri: true }
                route_config:
                  virtual_hosts:
                    - { name: hermes, domains: ["*"], routes: [ { match: { prefix: "/" }, route: { cluster: hermes } } ] }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config: { "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router }
  clusters:
    - name: hermes
      type: STATIC
      load_assignment:
        cluster_name: hermes
        endpoints: [ { lb_endpoints: [ { endpoint: { address: { pipe: { path: /run/hermes/api.sock } } } } ] } ]
```

Los recursos SDS apuntan a `current/tls.crt`, `current/tls.key` y `current/ca.crt`, con `watched_directory` en
`HERMES_PROXY_DIR`; el de validación lleva `trust_chain_verification: ACCEPT_UNTRUSTED`.

## La cabecera que lee Hermes

`X-Forwarded-Client-Cert` en el formato de texto de Envoy. Con `SANITIZE_SET` hay un solo elemento, con estos campos
separados por `;`:

| Campo | Regla de Hermes |
|---|---|
| `Hash` | Obligatorio: el SHA-256 en hexadecimal del certificado; tiene que coincidir con `Cert` |
| `Cert` | Obligatorio: el certificado del cliente en PEM con codificación URL, entre comillas |
| `Chain` | Obligatorio: la cadena que mandó el cliente, con el certificado y su intermedia, en PEM con codificación URL, entre comillas |
| `URI` | Obligatorio y único: el SPIFFE ID del certificado |
| `By` | Si viene, el SPIFFE ID de Hermes |

- Sin cabecera, con más de un elemento, sin uno de los campos obligatorios o con un valor que no se decodifica, las
  rutas autenticadas responden `auth`.
- La cadena que trae `Chain` no está validada por Envoy: Hermes la valida entera ([api.md](api.md#autenticación)).
