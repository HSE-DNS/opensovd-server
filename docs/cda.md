# Documentation: Classic Diagnostic Adapter (CDA) & OpenSOVD Setup

---

## 1. Introduction: What is the CDA?

The **Classic Diagnostic Adapter (CDA)** is a core component within the Eclipse OpenSOVD ecosystem. It serves as an abstraction layer (or translator) between the modern, IT-based world of SOVD (Service-Oriented Vehicle Diagnostics) and the classic vehicle diagnostic infrastructure.

### The Challenge

Classic Electronic Control Units (ECUs) communicate using protocols like:

- UDS (Unified Diagnostic Services)  
- DoIP (Diagnostics over IP)  

These protocols are highly **binary, stateful, and hardware-oriented**.

### The Solution

The CDA:

1. Receives HTTP/REST requests in standardized SOVD format  
2. Uses its **ODX database** to interpret the request  
3. Translates it into classic **UDS payloads**  
4. Sends it to the vehicle or simulator  
5. Converts the binary response into **readable JSON**

---

## 2. Configuration of the CDA

The CDA is primarily configured using a TOML file (e.g., `cda-test-config.toml`).

### Key Configuration Sections

#### `[security]`

- `enabled = true` → Enables token-based authentication  
- `validation_key` → Secret key for signing and validating JWT tokens  

#### `[database]`

- Defines the directory containing **ODX/PDX files**  
- Without these files, the CDA **cannot interpret ECU data**

#### `[[devices]]`

Defines target ECUs:

- `name` → Identifier (e.g., `flxc1000`)  
- `logical_address` → UDS address (e.g., `4096`)  
- `ip_address` → ECU / DoIP endpoint  

---

###  Important Deployment Note

> When mounting the configuration file into a Docker container via `docker-compose.yml`, the file must already exist on the host system.  
> If the file is missing, Docker will create a **directory instead of a file**, causing the CDA container to crash with an `"Is a directory"` error.

---

## 3. Authentication Flow (Token Handling)

The CDA implements an **OAuth2 authorization server**.

Static passwords are **not used** — instead, an **Access Token** must be requested.

### Acquiring the Access Token

```bash
curl -s -X POST "http://localhost:20002/vehicle/v15/authorize" \
     -H "Content-Type: application/json" \
     -d '{"client_id":"test", "client_secret":"secret"}'
```

### Response

- Returns a JSON object containing an `access_token`
- This token must be:
  - passed to the OpenSOVD Gateway at startup **or**
  - included in HTTP headers for direct CDA requests

---

## 4. Step-by-Step Installation & Execution Guide (Raspberry Pi Setup)

This setup uses:

- Native OpenSOVD Gateway binary  
- Dockerized CDA  
- ECU simulation  

---

### Step 1: ODX Data Preparation

Place diagnostic files (`.pdx`, `.odx-d`) in the database directory:

```bash
# Example path
~/classic-diagnostic-adapter/testcontainer/odx/
```

**Note:**  
If this directory is empty, the CDA will return:

```json
{"items":[]}
```

---

### Step 2: Start the Docker Infrastructure

```bash
cd ~/classic-diagnostic-adapter/testcontainer
docker compose up -d
```

Check status:

```bash
docker compose ps
```

---

### Step 3: Prepare the Gateway Binary

Ensure:

```bash
chmod +x opensovd-gateway
```

Binary location example:

```
~/artifact-test/
```

---

### Step 4: Automated Startup via Script

A script (`starter.sh`) automates:

- Waiting for containers  
- Fetching OAuth2 token  
- Starting the Gateway  

#### Core Script Logic

```bash
# 1. Retrieve the token
export ACCESS_TOKEN=$(curl -s -X POST ... | jq -r .access_token)

# 2. Start the native Gateway with the token
CDA_TOKEN="$ACCESS_TOKEN" ./opensovd-gateway \
  --url http://0.0.0.0:7690/sovd \
  --cda-host localhost \
  --cda-port 20002 \
  --cda-base-path "/vehicle/v15" &
```

#### Execution

```bash
./starter.sh start
```

---

## 5. Data Querying Examples

Once running, the OpenSOVD Gateway exposes data via HTTP (Port `7690`).

### List all data endpoints

```powershell
curl http://<SERVER_IP>:7690/sovd/v1/components/flxc1000/data
```

---

### Read specific data value

```powershell
curl http://<SERVER_IP>:7690/sovd/v1/components/flxc1000/data/FluxCapacitorPowerConsumption
```

---

## 6. Troubleshooting & Common Errors

---

### Error: Address already in use (os error 98)

**Cause:**  
A previous Gateway instance is still running.

**Fix:**

```bash
fuser -k 7690/tcp
# or
killall opensovd-gateway
```

---

### Response: `{"items":[]}`

**Cause A:**  
ODX/PDX directory is empty  

**Cause B:**  
ECU variant not yet detected  

**Fix:**  
Wait a few seconds after startup

---

### Error: InvalidToken (CDA logs)

**Cause:**

- Token does not match `validation_key`  
- Security settings mismatch between Gateway and CDA  

---