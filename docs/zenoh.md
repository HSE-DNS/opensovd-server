# Documentation: OpenSOVD Zenoh Integration

This documentation describes the integration of Zenoh as a data source (provider) into the OpenSOVD Gateway. It serves as a guide for operation, configuration, and understanding of the system architecture.

# Zenoh Provider – Architecture and Functionality

## What is the Zenoh Provider and how does it work?

The Zenoh Provider acts as an intelligent translator (adapter) between the standardized OpenSOVD world and the flexible Zenoh Pub/Sub network.  

While OpenSOVD strictly operates with structured concepts like **Components** and **Data IDs** and communicates via HTTP requests, Zenoh uses hierarchical paths (called **Key Expressions**, e.g. `my-vehicle/battery/soc`) and asynchronous messaging.

The Provider bridges this gap in three essential phases:

- Initialization  
- Discovery  
- Data Access (Read/Write operations)  

---

## 1. Connection Setup (Initialization)

As soon as the gateway starts, the `ZenohProvider` uses the parameters defined in `ZenohConfig` (such as IP address and port) to connect to the Zenoh router as a client.

It establishes a persistent session:

```rust
zenoh::open(...)
```

Through this session, all future data communication is handled.

---

## 2. Automatic Discovery

To identify which robots and sensors exist in the network, the Provider performs a network-wide query using the `discover` function.

It sends a generic selector such as:

```
**
```

This means "discover everything" in the Zenoh network.

### Process:

1. The Provider sends the query
2. Active Zenoh nodes (`z_queryable`) respond
3. The Provider analyzes the returned paths

Each path is processed as follows:

- Split by `/`
- Extract robot name using `robot_name_index`
- Convert remaining path into a Data ID using `_`

### Example:

```
my-vehicle/battery/soc
```

Becomes:

- **Component:** `my-vehicle`
- **Data ID:** `battery_soc`



---

## 3. Read and Write Access (Data Access)

For each discovered component, the gateway creates a dedicated `ZenohDataProvider`.

This provider is responsible for real-time communication.

---

### Read (GET)

When a client requests data via the SOVD REST API:

1. The Provider receives a Data ID (e.g. `battery_soc`)
2. Converts `_` back to `/`
3. Queries Zenoh using:

```rust
session.get(...)
```

4. Receives the response (payload)
5. Parses it as JSON

#### Special Case: Raw Data

If the response contains plain values (e.g. numbers or strings), it is automatically wrapped into:

```json
{ "raw_value": ... }
```

This ensures compatibility with the SOVD standard.

---

### Write (PUT)

When sending commands via HTTP PUT:

1. The Provider extracts the value from the JSON request
2. Converts the Data ID into a Zenoh key
3. Publishes the value using:

```rust
session.put(...)
```

4. The robot receives the update immediately and can react accordingly

---

## Summary

The Zenoh Provider enables seamless integration between:

- **Zenoh (Pub/Sub, async, path-based)**
- **OpenSOVD (REST, structured, synchronous)**

by dynamically mapping data, discovering network entities, and translating communication in real time.

---

## 1. System Overview & Architecture

The system consists of three main components that work seamlessly together:

- **Zenoh Router / Daemon**  
  The central message broker (or infrastructure) through which all robot data flows.

- **Zenoh Provider (Gateway Module)**  
  A specialized driver within the OpenSOVD Gateway that translates the Zenoh world (Pub/Sub) into the SOVD world (RESTful API).

- **SOVD Server**  
  The external interface that provides data via a standardized HTTP API for clients (apps, diagnostic tools).

---

## 2. Functionality of the Zenoh Provider

The Zenoh Provider implements two core OpenSOVD interfaces:

### A. Discovery

At startup (or on trigger), the provider scans the Zenoh network using a selector (e.g., `**`).  
It parses the discovered Zenoh paths (keys) and groups them into components (robots).

**Example:**

From the key  
`RobotA/battery/voltage`  
the provider identifies:

- Component: `RobotA`
- Data point: `battery_voltage`

---

### B. Data Access

#### Read (GET)

When a user calls an SOVD URL:

- The provider sends a GET request into the Zenoh network
- A program running on the robot (**Queryable**) responds with the current value

#### Write (PUT)

When a user sends a new value:

- The provider performs a PUT on the corresponding Zenoh key
- The robot receives the update immediately

---

## 3. Quick Start Guide

### Step 1: Simulate Zenoh Data Source (Test Terminal)

Before starting the gateway, a data point must exist in the Zenoh network.

```bash
# Starts a data point that responds to queries (Queryable)
./target/release/examples/z_queryable \
  --key "my-vehicle/battery/soc" \
  --payload '{ "value": 99 }' \
  --connect tcp/localhost:7447
```

Optional: Start a subscriber to observe write operations (PUT) in real time:

```bash
./target/release/examples/z_sub \
  --key "my-vehicle/battery/soc" \
  --connect tcp/localhost:7447
```

---

### Step 2: Start OpenSOVD Gateway

```bash
# Starts the gateway and connects it to the local Zenoh router
cargo run -p opensovd-gateway -- --url http://0.0.0.0:7690/sovd
```

---

## 4. API Usage (Examples)

Once the gateway is running, data can be accessed via standard HTTP commands.

### Retrieve Data (Read)

```bash
# List all discovered components
curl -s http://localhost:7690/sovd/v1/components | jq

# List all data points of a component
curl -s http://localhost:7690/sovd/v1/components/my-vehicle/data | jq

# Retrieve a specific value
curl -s http://localhost:7690/sovd/v1/components/my-vehicle/data/battery_soc | jq
```

---

### Modify Data (Write)

```bash
curl -i -X PUT -H "Content-Type: application/json" \
     -d '{"data": {"value": 42}}' \
     http://localhost:7690/sovd/v1/components/my-vehicle/data/battery_soc
```

---

## 5. 🛠 Configuration Guide (Robot Day)

To adapt the gateway to different robot environments, no complex code changes are required.  
All settings are centrally defined in `ZenohConfig` (in `main.rs`).

### Key Parameters

| Parameter           | Description                                  | Example                  |
|--------------------|----------------------------------------------|--------------------------|
| endpoint           | Address of the Zenoh router in the network   | "tcp/192.168.1.50:7447"  |
| discovery_selector | Filter for scanning Zenoh keys               | "robots/**"              |
| robot_name_index   | Index of the robot name in the path          | 0                        |

---

### Scenario: Hierarchical Paths

Given:

```
Werk_Süd/Halle_1/Robot_Alpha/sensor/temp
```

Configuration:

- **Selector:** `"Werk_Süd/Halle_1/**"`
- **Index:** `2` → results in `Robot_Alpha` as the SOVD component

```rust
// Configuration example in main.rs
let zenoh_config = ZenohConfig {
    endpoint: "tcp/10.42.0.1:7447".to_string(),
    discovery_selector: "Werk_Süd/Halle_1/**".to_string(),
    robot_name_index: 2,
};
```

---

## 6. Technical Details for Developers

### Dynamic Mapping

- **Path Conversion:**  
  Zenoh uses `/`, SOVD uses `_`  
  → automatic conversion  
  `battery/soc ↔ battery_soc`

- **Payloads:**  
  - Expected: JSON  
  - Raw data → automatically wrapped into:

```json
{ "raw_value": ... }
```

---

### Error Handling

- **404 – Entity Not Found**  
  - Robot not discovered during startup  
  - Zenoh router not reachable  

- **500 – Internal Error**  
  - Network timeout  
  - Zenoh daemon crashed  

---

## 🔗 Further Resources

- Zenoh Official Examples (GitHub) – Useful for simulating robots [examples](https://github.com/eclipse-zenoh/zenoh/tree/main/examples/examples) Stand 23.03.2026


