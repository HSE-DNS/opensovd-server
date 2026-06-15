use anyhow::anyhow;
use async_trait::async_trait;
use opensovd_core::{
    Component, Data, DataError, DataFilter, DataProvider, DiscoveryError, DiscoveryProvider,
    EntityCollection, EntityRef, Metadata,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::pin::Pin;
use zenoh::Session;

pub struct ZenohConfig {  //opensovd-cli/src/main.rs
    /// The network address of the Zenoh Router (e.g., "127.0.0.1:7447" or "192.168.1.50:7447")
    pub endpoint: String,
    /// The Zenoh selector used to find robots.
    /// Use "**" for everything or "robots/**" to filter for specific prefixes.
    pub discovery_selector: String,
    /// Defines which part of the Zenoh path is the Robot Name.
    /// Index 0 means the first part (e.g., "RobotA/sensor" -> "RobotA")
    pub robot_name_index: usize,
    /// Fallback category when the queryable response does not include one.
    pub category: String,
}

impl Default for ZenohConfig {
    fn default() -> Self {
        Self {
            endpoint: "tcp/localhost:7447".to_string(),
            discovery_selector: "**".to_string(),
            robot_name_index: 0,
            category: "currentData".to_string(),
        }
    }
}

pub struct ZenohProvider {
    session: Session,
    config: ZenohConfig,
}

impl ZenohProvider {
    pub async fn new(config: ZenohConfig) -> anyhow::Result<Self> {
        let mut zenoh_config = zenoh::Config::default();
        zenoh_config.insert_json5("mode", r#""client""#).map_err(|e| anyhow!("{e}"))?;
        let endpoints_json = format!(r#"["{}"]"#, config.endpoint);
        zenoh_config.insert_json5("connect/endpoints", &endpoints_json).map_err(|e| anyhow!("{e}"))?;
        let session = zenoh::open(zenoh_config).await.map_err(|e| anyhow!("{e}"))?;
        tracing::info!("ZenohProvider connected to {}", config.endpoint);
        Ok(Self { session, config })
    }
}

struct DataPointMeta {
    id: String,
    name: String,
    category: String,
}

#[async_trait]
impl DiscoveryProvider for ZenohProvider {
    async fn discover(
        &self,
    ) -> Result<
        Pin<Box<dyn futures::stream::Stream<Item = Result<(Vec<EntityRef>, EntityCollection), DiscoveryError>> + Send + 'static>>,
        DiscoveryError,
    > {
        let mut collection = EntityCollection::default();

        tracing::info!("Starting discovery with selector: {}", self.config.discovery_selector);

        let replies = self.session.get(&self.config.discovery_selector)
            .await
            .map_err(|e| DiscoveryError::Other(e.to_string().into()))?;

        // robot_name -> (data_id -> DataPointMeta)
        let mut robot_map: HashMap<String, HashMap<String, DataPointMeta>> = HashMap::new();

        while let Ok(reply) = replies.recv_async().await {
            if let Ok(sample) = reply.result() {
                let key = sample.key_expr().as_str();

                if key.contains('*') {
                    continue;
                }

                let parts: Vec<&str> = key.split('/').collect();

                if let Some(robot_name) = parts.get(self.config.robot_name_index) {
                    let robot_name = robot_name.to_string();
                    let prefix = format!("{}/", robot_name);
                    let relative_key = key.strip_prefix(&prefix).unwrap_or(key);
                    let data_id = relative_key.replace('/', "_");

                    if data_id.is_empty() {
                        continue;
                    }

                    // Parse the ZenohQuery envelope to extract name and category.
                    // Falls back to a derived name and the config default if the payload
                    // does not contain these fields.
                    let payload_str = String::from_utf8_lossy(&sample.payload().to_bytes()).into_owned();
                    let json_payload: Option<Value> = serde_json::from_str(&payload_str).ok();

                    let display_name = json_payload.as_ref()
                        .and_then(|v| v.get("name"))
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .unwrap_or_else(|| data_id.replace('_', " "));

                    let category = json_payload.as_ref()
                        .and_then(|v| v.get("category"))
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .unwrap_or_else(|| self.config.category.clone());

                    tracing::info!(
                        "Discovered: robot='{}', id='{}', name='{}', category='{}'",
                        robot_name, data_id, display_name, category
                    );

                    robot_map
                        .entry(robot_name)
                        .or_default()
                        .insert(data_id.clone(), DataPointMeta { id: data_id, name: display_name, category });
                }
            }
        }

        for (robot_name, points) in robot_map {
            tracing::info!("Component created: '{}' with {} data points", robot_name, points.len());
            let mut component = Component::new(robot_name.clone(), format!("Zenoh Robot {robot_name}"));

            component = component.with_data_provider(ZenohDataProvider {
                session: self.session.clone(),
                robot_name: robot_name.clone(),
                data_points: points.into_values().collect(),
                fallback_category: self.config.category.clone(),
            });
            collection.components.push(component);
        }

        let stream = futures::stream::once(std::future::ready(Ok((vec![], collection))));
        Ok(Box::pin(stream))
    }
}


pub struct ZenohDataProvider {
    session: Session,
    robot_name: String,
    data_points: Vec<DataPointMeta>,
    fallback_category: String,
}

#[async_trait]
impl DataProvider for ZenohDataProvider {
    async fn list(&self, _filter: DataFilter) -> Result<Vec<Metadata>, DataError> {
        let mut metadata = vec![Metadata {
            id: "telemetry".to_string(),
            name: format!("{} All Telemetry", self.robot_name),
            category: self.fallback_category.clone(),
            is_readable: true,
            is_writable: false,
            translation_id: None,
            groups: vec![],
            tags: vec![],
            schema: None,
        }];

        for dp in &self.data_points {
            metadata.push(Metadata {
                id: dp.id.clone(),
                name: dp.name.clone(),
                category: dp.category.clone(),
                is_readable: true,
                is_writable: false,
                translation_id: None,
                groups: vec![],
                tags: vec![],
                schema: None,
            });
        }

        Ok(metadata)
    }

    async fn read(&self, data_id: &str, _include_schema: bool) -> Result<Data, DataError> {
        let key = if data_id == "telemetry" {
            format!("{}/**", self.robot_name)
        } else if self.data_points.iter().any(|dp| dp.id == data_id) {
            let zenoh_path = data_id.replace('_', "/");
            format!("{}/{}", self.robot_name, zenoh_path)
        } else {
            return Err(DataError::NotFound(data_id.to_string()));
        };

        let replies = self.session.get(&key).await.map_err(|e| DataError::Internal(e.to_string()))?;

        let mut data_map = serde_json::Map::new();
        let mut found_any = false;

        while let Ok(reply) = replies.recv_async().await {
            if let Ok(sample) = reply.result() {
                found_any = true;
                let full_key = sample.key_expr().as_str();
                let prefix = format!("{}/", self.robot_name);
                let relative_key = full_key.strip_prefix(&prefix).unwrap_or(full_key);

                let body = String::from_utf8_lossy(&sample.payload().to_bytes()).into_owned();
                let json_payload: Value = serde_json::from_str(&body)
                    .unwrap_or_else(|_| json!({ "raw_value": body }));

                // Extract only the data field from the ZenohQuery envelope.
                // If the payload has no "data" field, use the whole payload as-is.
                let val = json_payload.get("data").cloned().unwrap_or(json_payload);

                let json_key = relative_key.replace('/', "_");
                data_map.insert(json_key, val);
            }
        }

        if !found_any && data_id != "telemetry" {
            return Err(DataError::NotFound(format!("No data found for {data_id} in Zenoh")));
        }

        let payload = if data_id == "telemetry" {
            Value::Object(data_map)
        } else {
            data_map.get(data_id).cloned().unwrap_or_else(|| json!(data_map))
        };

        Ok(Data { data: payload, schema: None })
    }

    async fn write(&self, _data_id: &str, _value: Value) -> Result<(), DataError> {
        Err(DataError::Internal(
            "Writing to Zenoh queryables is not supported".to_string(),
        ))
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use opensovd_core::{DataFilter, DataProvider};
    use serde_json::json;

    #[test]
    fn test_zenoh_config_default() {
        let config = ZenohConfig::default();
        assert_eq!(config.endpoint, "tcp/localhost:7447");
        assert_eq!(config.discovery_selector, "**");
        assert_eq!(config.robot_name_index, 0);
        assert_eq!(config.category, "currentData");
    }

    #[tokio::test]
    async fn test_zenoh_provider_list_metadata() {
        let config = zenoh::Config::default(); // Lokaler, temporärer Peer-Modus für Tests
        let session = zenoh::open(config).await.unwrap();
        
        let provider = ZenohDataProvider {
            session,
            robot_name: "TestRobot".to_string(),
            data_points: vec![DataPointMeta {
                id: "speed_sensor".to_string(),
                name: "Speed Sensor".to_string(),
                category: "sensorData".to_string(),
            }],
            fallback_category: "currentData".to_string(),
        };

        let metadata = provider.list(DataFilter::default()).await.unwrap();
        
        // expecting 2 entries (telemetry + speed_sensor)
        assert_eq!(metadata.len(), 2);
        assert_eq!(metadata[0].id, "telemetry");
        assert_eq!(metadata[1].id, "speed_sensor");
        assert_eq!(metadata[1].category, "sensorData");
    }

    #[tokio::test]
    async fn test_zenoh_provider_write_fails() {
        let config = zenoh::Config::default();
        let session = zenoh::open(config).await.unwrap();
        let provider = ZenohDataProvider { session, robot_name: "R1".to_string(), data_points: vec![], fallback_category: "".to_string() };

        let result = provider.write("any_id", json!("some_value")).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not supported"));
    }
}