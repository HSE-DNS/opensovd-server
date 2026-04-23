# OpenSOVD Gateway & CDA Provider Integration

---

## 1. Concept & Architecture

The OpenSOVD Gateway, written in Rust, has been extended with a specific CDA provider (`cda.rs`). This provider acts as an intelligent middleware between the standardized SOVD interface (REST API for the end user) and the Classic Diagnostic Adapter (CDA).

### Data Flow

Client → OpenSOVD Gateway → CDA Provider → CDA Server → ODX Database → UDS/DoIP → ECU

### Advantage

The gateway abstracts:

- Complex token handling  
- CDA-specific path structures  

The user only interacts with a clean, standardized SOVD API.

---

## 2. Configuration & Required Data Types

To successfully instantiate the `CdaProvider`, specific configuration parameters are required. In Rust, this is handled via the `new()` function.

### Required Input Parameters

#### host (Type: String)

The IP address or hostname of the CDA server.

Examples:

```
"127.0.0.1"
"localhost"
"host.docker.internal"
```

---

#### port (Type: u16 – Unsigned 16-bit Integer)

The port on which the CDA server is listening.

Default:

```
20002
```

---

#### base_path (Type: String)

The base path of the CDA API.

Example:

```
"/vehicle/v15"
```

---

#### token (Type: String)

A valid JWT (JSON Web Token) used for authentication with the CDA.  
It is passed in the HTTP header as a Bearer token.

---

## 3. Implementation Details (`cda.rs`)

The provider is divided into two logical Rust implementations to comply with the requirements of the `opensovd_core` library.

---

### DiscoveryProvider (`CdaProvider`)

Responsible for dynamically scanning the vehicle topology at server startup.  
Nothing is hardcoded.

#### Component Discovery

At startup, an HTTP GET request is sent to:

```
{host}:{port}{base_path}/components
```

The CDA returns a JSON list of all detected ECUs (e.g., `flxc1000`).

---

#### Data Point Discovery

For each discovered component, the provider immediately queries its available diagnostic data:

```
/components/flxc1000/data
```

---

#### Storage

The discovered data points are stored in memory as a list of tuples:

```
(String, String)
```

(ID and name)

Based on this data, a `CdaDataProvider` instance is dynamically created for each component.

---

### DataProvider (`CdaDataProvider`)

Bound to a specific component (e.g., `flxc1000`) and responsible for handling user requests.

---

#### list() Method

Returns the discovered data points (from the discovery process) as structured:

```
opensovd_core::Metadata
```

---

#### read() Method

Core read functionality.

When a specific value (e.g., `VINDataIdentifier`) is requested:

1. The target URL is dynamically constructed  
2. An HTTP GET request is sent to the CDA  

##### Special Behavior

The method processes CDA-specific JSON structures and extracts the inner `"data"` field (if present) to return a clean result.

---

#### write() Method

Allows writing values to the ECU.

- Sends an HTTP PUT request to the CDA  
- The value (`serde_json::Value`) is automatically wrapped into:

```json
{"data": value}
```

This format is required by the CDA.

---

## 4. Security & Dynamic Behavior

### Full Flexibility

- No static data points  
- The gateway automatically supports all diagnostic commands defined in the ODX file at runtime  

---

### Secure Token Handling

- JWT is **not stored in source code**  
- Loaded dynamically from environment variable:

```
CDA_TOKEN
```

---

### Dynamic Routing

Environment variables allow flexible configuration:

- `CDA_HOST`  
- `CDA_PORT`  
- `CDA_BASE_PATH`  

This enables usage across different environments:

- Local  
- Test server  
- Raspberry Pi  

No recompilation required.

---

## 5. Operation & Testing Commands

---

### Preparation: Determine IP Address

If the CDA runs in Docker, the correct IP must be determined.

```bash
# Show default gateway (host) IP
ip route | awk '/default/ { print $3 }'

# Get specific Docker container IP
docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' cda-1
```

---

### 1. Start Gateway

The gateway is started via `cargo` or as a compiled binary.  
Configuration is injected via environment variables.

---

#### Run against localhost

```bash
CDA_HOST="127.0.0.1" \
CDA_TOKEN="eyJ0eXAiOiJKV1Qi..." \
cargo run -p opensovd-gateway -- --url http://0.0.0.0:7690/sovd
```

---

#### Run against Docker container (recommended for dev setups)

```bash
CDA_HOST="host.docker.internal" \
CDA_TOKEN="eyJ0eXAiOiJKV1Qi..." \
cargo run -p opensovd-gateway -- --url http://0.0.0.0:7690/sovd
```

---

### 2. Query Data (REST API)

#### List all components

```bash
curl -s http://127.0.0.1:7690/sovd/v1/components | jq
```

---

#### List data of a specific component

```bash
curl -s http://127.0.0.1:7690/sovd/v1/components/flxc1000/data | jq
```

---

#### Read live data point (e.g., VIN)

```bash
curl -s http://127.0.0.1:7690/sovd/v1/components/flxc1000/data/VINDataIdentifier | jq
```

---

## 6. Devcontainer Optimization (Docker-to-Docker)

To simplify networking between the devcontainer (where the gateway runs) and the CDA container, the DNS name `host.docker.internal` should be made globally available.

Add the following to `.devcontainer/devcontainer.json`:

```json
"runArgs": [
    "--add-host=host.docker.internal:host-gateway"
]
```

After rebuilding the container:

- `host.docker.internal` resolves reliably to the host IP (usually `172.17.0.1`)  
- Manual IP lookup is no longer required  

Recommended for **testing environments only**