use anyhow::anyhow;
use async_trait::async_trait;
use opensovd_core::{
    Component, Data, DataError, DataFilter, DataProvider, DiscoveryError, DiscoveryProvider,
    EntityCollection, EntityRef, Metadata,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use zenoh::Session;

/// CENTRAL CONFIGURATION
/// Change these values to adapt to your specific robot environment.
pub struct ZenohConfig {  //opensovd-cli/src/main.rs
    /// The network address of the Zenoh Router (e.g., "127.0.0.1:7447" or "192.168.1.50:7447")
    pub endpoint: String,
    /// The Zenoh selector used to find robots. 
    /// Use "**" for everything or "robots/**" to filter for specific prefixes.
    pub discovery_selector: String,
    /// Defines which part of the Zenoh path is the Robot Name.
    /// Index 0 means the first part (e.g., "RobotA/sensor" -> "RobotA")
    pub robot_name_index: usize,
}

impl Default for ZenohConfig {
    fn default() -> Self {
        Self {
            endpoint: "tcp/localhost:7447".to_string(),
            discovery_selector: "**".to_string(),
            robot_name_index: 0,
        }
    }
}

pub struct ZenohProvider {
    session: Session,
    config: ZenohConfig,
}

impl ZenohProvider {
    /// Initializes a new `ZenohProvider` using a central config.
    ///
    /// # Errors
    ///
    /// Returns an error if the Zenoh configuration is invalid or the 
    /// connection to the Zenoh router fails.
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

#[async_trait]
impl DiscoveryProvider for ZenohProvider {
    /// Discovers robots and their data points in the Zenoh network.
    ///
    /// # Errors
    ///
    /// Returns a `DiscoveryError::Other` if the Zenoh session fails to 
    /// perform the GET request or if the network communication is interrupted.
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

        let mut robot_map: HashMap<String, HashSet<String>> = HashMap::new();

        while let Ok(reply) = replies.recv_async().await {
            if let Ok(sample) = reply.result() {
                let key = sample.key_expr().as_str();
                let parts: Vec<&str> = key.split('/').collect();
                
                // Determine the robot name based on the index in the config
                if let Some(robot_name) = parts.get(self.config.robot_name_index) {
                    let prefix = format!("{}/", robot_name);
                    let relative_key = key.strip_prefix(&prefix).unwrap_or(key);
                    let data_id = relative_key.replace('/', "_");
                    
                    robot_map.entry(robot_name.to_string()).or_default().insert(data_id);
                }
            }
        }

        for (robot_name, data_ids) in robot_map {
            tracing::info!("Component created: '{}'", robot_name);
            let mut component = Component::new(robot_name.clone(), format!("Zenoh Robot {robot_name}"));
            
            component = component.with_data_provider(ZenohDataProvider {
                session: self.session.clone(),
                robot_name: robot_name.clone(),
                data_points: data_ids.into_iter().collect(),
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
    data_points: Vec<String>,
}

#[async_trait]
impl DataProvider for ZenohDataProvider {
    async fn list(&self, _filter: DataFilter) -> Result<Vec<Metadata>, DataError> {
        let mut metadata = vec![Metadata {
            id: "telemetry".to_string(),
            name: format!("{} All Telemetry", self.robot_name),
            category: "Zenoh-Telemetry".to_string(),
            is_readable: true,
            is_writable: false,
            translation_id: None,
            groups: vec![],
            tags: vec![],
            schema: None,
        }];

        for id in &self.data_points {
            if id.is_empty() { continue; }
            metadata.push(Metadata {
                id: id.clone(),
                name: id.replace('_', " "),
                category: "Zenoh-Telemetry".to_string(),
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
        } else if self.data_points.contains(&data_id.to_string()) {
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
                let val: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!(body));
                
                let json_key = relative_key.replace('/', "_");
                data_map.insert(json_key, val);
            }
        }

        if !found_any && data_id != "telemetry" {
            return Err(DataError::NotFound(format!("No data found for {data_id} in Zenoh")));
        }

        let payload = json!({
            "name": self.robot_name,
            "category": "Zenoh-Telemetry",
            "data": data_map
        });

        Ok(Data { data: payload, schema: None })
    }

    async fn write(&self, _data_id: &str, _value: Value) -> Result<(), DataError> {
        Err(DataError::Internal("Writing to aggregated telemetry is not supported directly".to_string()))
    }
}